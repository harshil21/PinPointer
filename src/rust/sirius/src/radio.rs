//! Non-blocking LoRa radio driver for Sirius.
//!
//! # Design
//!
//! The radio runs in **continuous RX mode** almost all of the time.
//! A downlink telemetry packet is transmitted every [`TX_INTERVAL`] by
//! briefly interrupting continuous RX, calling `radio.transmit()`, then
//! immediately restarting continuous RX.
//!
//! [`poll_receive`](rfm95::Rfm95::poll_receive) is called on every loop
//! iteration and returns `Ok(None)` immediately if no packet has arrived,
//! so the thread never blocks waiting for data.
//!
//! # RTK fragmentation
//!
//! When sopdet has more RTCM data than fits in one [`UplinkPacket`]
//! (max [`MAX_UPLINK_RTK`] bytes), it splits the batch into consecutive
//! [`RtcmFragment`] packets. [`FragmentAssembler`] collects them by
//! `session_id` and forwards the reassembled buffer to the main thread
//! (which writes it to the LC29H) as soon as every fragment is present.
//! Incomplete sessions are discarded after [`FRAG_TIMEOUT`].
//!
//! # Command signalling
//!
//! Ground-station commands that require action in the main thread are
//! signalled via [`Arc<AtomicBool>`]:
//!
//! * `emergency_flag` — set when `EmergencyLocate` is received; never cleared.
//! * `deploy_flag`    — set when `DeployEjectionCharge` is received; cleared
//!                      by the main thread after the pyro has fired.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rfm95::{Bandwidth, LoraConfig, Rfm95, Rfm95Error, SpreadingFactor};

use protocol::{
    DEBUG_TYPE, DOWNLINK_TYPE, DebugDownlinkPacket, DownlinkPacket, FRAG_TYPE, GroundCommand,
    RtcmFragment, UPLINK_TYPE, UplinkPacket,
};

use crate::data_processor::FlightData;

// ── Configuration ─────────────────────────────────────────────────────────────

/// How often sirius transmits a telemetry downlink.
///
/// With SF7 / BW500 a 43-byte packet takes ≈ 8 ms on-air, leaving
/// ~192 ms of continuous RX time for sopdet to send uplinks and fragments.
const TX_INTERVAL: Duration = Duration::from_millis(200);

/// Incomplete fragment sessions are discarded after this timeout.
const FRAG_TIMEOUT: Duration = Duration::from_secs(3);

/// Sleep between poll iterations (yields the CPU without burning it).
const POLL_SLEEP: Duration = Duration::from_millis(1);

// ── Initial radio configuration ───────────────────────────────────────────────

/// SF7 / BW500 — high data rate, short on-air time.
fn initial_config() -> LoraConfig {
    LoraConfig {
        spreading_factor: SpreadingFactor::Sf7,
        bandwidth: Bandwidth::Bw500kHz,
        ..LoraConfig::default()
    }
}

// ── Fragment assembler ────────────────────────────────────────────────────────

/// Reassembles a fragmented RTCM batch from consecutive [`RtcmFragment`]
/// packets.
///
/// A new session begins when a fragment with a different `session_id` arrives.
/// Any incomplete previous session is silently discarded at that point.
pub struct FragmentAssembler {
    /// Active session identifier; `0xFF` means no session is in progress.
    session_id: u8,
    total_frags: u8,
    slots: Vec<Option<Vec<u8>>>,
    filled: u8,
    last_recv: Option<Instant>,
}

impl FragmentAssembler {
    pub fn new() -> Self {
        FragmentAssembler {
            session_id: 0xFF,
            total_frags: 0,
            slots: Vec::new(),
            filled: 0,
            last_recv: None,
        }
    }

    /// Feed one fragment.
    ///
    /// Returns the complete reassembled payload when all fragments of the
    /// current session have been received; `None` otherwise.
    pub fn add(&mut self, frag: RtcmFragment) -> Option<Vec<u8>> {
        // Start a fresh session if the session_id changed.
        if frag.session_id != self.session_id || frag.total_frags != self.total_frags {
            if self.session_id != 0xFF {
                log::debug!(
                    "[frag] Discarding incomplete session {} ({}/{} received)",
                    self.session_id,
                    self.filled,
                    self.total_frags,
                );
            }
            self.session_id = frag.session_id;
            self.total_frags = frag.total_frags;
            self.slots = vec![None; frag.total_frags as usize];
            self.filled = 0;
            self.last_recv = None;
            log::debug!(
                "[frag] New session {} — expecting {} fragments",
                frag.session_id,
                frag.total_frags,
            );
        }

        let idx = frag.frag_index as usize;
        if idx >= self.slots.len() {
            log::warn!(
                "[frag] Fragment index {} out of range (total={})",
                frag.frag_index,
                self.total_frags,
            );
            return None;
        }

        if self.slots[idx].is_none() {
            self.slots[idx] = Some(frag.data);
            self.filled += 1;
            self.last_recv = Some(Instant::now());
            log::debug!(
                "[frag] session={} [{}/{}]",
                self.session_id,
                self.filled,
                self.total_frags,
            );
        }

        if self.filled as usize == self.slots.len() {
            let assembled: Vec<u8> = self
                .slots
                .iter()
                .filter_map(|s| s.as_deref())
                .flat_map(|d| d.iter().copied())
                .collect();
            log::info!(
                "[frag] Complete: session={} {} bytes ({} frags)",
                self.session_id,
                assembled.len(),
                self.total_frags,
            );
            self.reset();
            return Some(assembled);
        }

        None
    }

    /// True when a session has been started but not completed within
    /// [`FRAG_TIMEOUT`].
    pub fn is_timed_out(&self) -> bool {
        self.last_recv
            .map(|t| t.elapsed() > FRAG_TIMEOUT)
            .unwrap_or(false)
    }

    /// Discard any in-progress assembly.
    pub fn reset(&mut self) {
        self.session_id = 0xFF;
        self.total_frags = 0;
        self.slots.clear();
        self.filled = 0;
        self.last_recv = None;
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Run the radio management loop (blocks forever — call from a dedicated thread).
///
/// # Arguments
///
/// * `radio`          — opened `Rfm95` instance (not yet configured).
/// * `flight_data`    — latest [`FlightData`] snapshot, updated by the main
///                      thread; radio reads it briefly before each TX.
/// * `rtk_tx`         — sends reassembled RTCM bytes to the main thread,
///                      which forwards them to the LC29H GPS module.
/// * `tx_log_tx`      — sends the raw bytes of each transmitted downlink to
///                      the main thread for CSV logging.
/// * `rx_log_tx`      — sends the raw bytes of each received uplink / final
///                      fragment to the main thread for CSV logging.
/// * `emergency_flag` — set to `true` when `EmergencyLocate` is received.
/// * `deploy_flag`    — set to `true` when `DeployEjectionCharge` is received.
/// * `boot`           — rocket boot instant, used for `timestamp_ms`.
/// Duration of continuous radio silence before `contact_lost_flag` is set.
const CONTACT_LOST_TIMEOUT: Duration = Duration::from_secs(5);

pub fn run_radio_thread(
    mut radio: Rfm95,
    flight_data: Arc<Mutex<FlightData>>,
    rtk_tx: mpsc::Sender<Vec<u8>>,
    tx_log_tx: mpsc::Sender<Vec<u8>>,
    rx_log_tx: mpsc::Sender<Vec<u8>>,
    emergency_flag: Arc<AtomicBool>,
    deploy_flag: Arc<AtomicBool>,
    contact_lost_flag: Arc<AtomicBool>,
    boot: Instant,
) {
    log::info!("[radio] Thread started");

    // Local debug-mode flag — toggled by EnableDebugTelemetry / DisableDebugTelemetry.
    let debug_mode = Arc::new(AtomicBool::new(false));

    // Apply SF7 / BW500 configuration.
    if let Err(e) = radio.configure(&initial_config()) {
        log::error!("[radio] Initial configure failed: {}", e);
        return;
    }

    // Enter continuous RX mode immediately.
    if let Err(e) = radio.start_receive_continuous() {
        log::error!("[radio] Failed to start continuous RX: {}", e);
        return;
    }

    let mut assembler = FragmentAssembler::new();
    let mut tx_sequence: u16 = 0;

    // Track last RX time for contact-lost detection.
    let mut last_rx: Option<Instant> = None;
    let mut had_contact = false;

    // Use a past instant so we transmit immediately on the first iteration.
    let mut last_tx = Instant::now()
        .checked_sub(TX_INTERVAL + Duration::from_millis(1))
        .unwrap_or_else(Instant::now);

    loop {
        // ── 1. Non-blocking RX poll ───────────────────────────────────────────
        match radio.poll_receive() {
            Ok(Some(pkt)) => {
                // Clear RX_DONE IRQ flag so subsequent polls don't re-read
                // the same packet.
                let _ = radio.clear_irq_flags();

                // Record contact and clear any stale contact-lost flag.
                had_contact = true;
                last_rx = Some(Instant::now());
                contact_lost_flag.store(false, Ordering::Relaxed);

                handle_received(
                    &pkt.payload,
                    &rtk_tx,
                    &rx_log_tx,
                    &emergency_flag,
                    &deploy_flag,
                    &mut assembler,
                );
            }

            Ok(None) => {
                // No packet yet — nothing to do.
            }

            Err(Rfm95Error::CrcError) => {
                let _ = radio.clear_irq_flags();
                log::debug!("[radio] RX CRC error");
            }

            Err(e) => {
                log::warn!("[radio] poll_receive error: {} — restarting RX", e);
                // Attempt to recover by re-entering continuous RX.
                let _ = radio.start_receive_continuous();
            }
        }

        // ── 2. Discard timed-out fragment session ─────────────────────────────
        if assembler.is_timed_out() {
            log::warn!("[radio] Fragment session timed out — discarding");
            assembler.reset();
        }

        // ── 3. Periodic downlink TX ───────────────────────────────────────────
        if last_tx.elapsed() >= TX_INTERVAL {
            transmit_downlink(&mut radio, &flight_data, &mut tx_sequence, &tx_log_tx, boot);
            // If debug mode is active, follow up with the debug packet.
            if debug_mode.load(Ordering::Relaxed) {
                transmit_debug(&mut radio, &flight_data, tx_sequence.wrapping_sub(1));
            }
            last_tx = Instant::now();
        }

        // ── 4. Contact-lost detection ─────────────────────────────────────────
        // Only raise the flag if we had previous contact — don't alarm on boot
        // before sopdet has started transmitting.
        if had_contact {
            let lost = last_rx
                .map(|t| t.elapsed() > CONTACT_LOST_TIMEOUT)
                .unwrap_or(false);
            contact_lost_flag.store(lost, Ordering::Relaxed);
        }

        thread::sleep(POLL_SLEEP);
    }
}

// ── Received packet handler ───────────────────────────────────────────────────

fn handle_received(
    payload: &[u8],
    rtk_tx: &mpsc::Sender<Vec<u8>>,
    rx_log_tx: &mpsc::Sender<Vec<u8>>,
    emergency_flag: &Arc<AtomicBool>,
    deploy_flag: &Arc<AtomicBool>,
    assembler: &mut FragmentAssembler,
) {
    if payload.is_empty() {
        return;
    }

    match payload[0] {
        // ── Standard uplink (command + optional small RTK payload) ────────────
        UPLINK_TYPE => {
            match UplinkPacket::deserialize(payload) {
                Some(uplink) => {
                    log::debug!(
                        "[radio] Uplink cmd={} rtk_len={}",
                        uplink.command,
                        uplink.rtk_data.len(),
                    );

                    // Forward any inline RTK data to the GPS module.
                    if !uplink.rtk_data.is_empty() {
                        let _ = rtk_tx.send(uplink.rtk_data);
                    }

                    // Handle commands.
                    match uplink.command {
                        GroundCommand::None => {}

                        GroundCommand::EmergencyLocate => {
                            if !emergency_flag.load(Ordering::Relaxed) {
                                log::warn!("[radio] EmergencyLocate command received!");
                            }
                            emergency_flag.store(true, Ordering::Relaxed);
                        }

                        GroundCommand::EmergencyLocateOff => {
                            log::info!("[radio] EmergencyLocate deactivated by ground station");
                            emergency_flag.store(false, Ordering::Relaxed);
                        }

                        GroundCommand::DeployEjectionCharge => {
                            log::warn!("[radio] DeployEjectionCharge command received!");
                            deploy_flag.store(true, Ordering::Relaxed);
                        }

                        GroundCommand::EnableDebugTelemetry => {
                            log::info!("[radio] Debug telemetry ENABLED");
                            debug_mode.store(true, Ordering::Relaxed);
                        }

                        GroundCommand::DisableDebugTelemetry => {
                            log::info!("[radio] Debug telemetry DISABLED");
                            debug_mode.store(false, Ordering::Relaxed);
                        }
                    }

                    // Log the raw bytes.
                    let _ = rx_log_tx.send(payload.to_vec());
                }

                None => {
                    log::warn!("[radio] Failed to parse uplink ({} bytes)", payload.len());
                }
            }
        }

        // ── RTCM fragment ─────────────────────────────────────────────────────
        FRAG_TYPE => {
            match RtcmFragment::deserialize(payload) {
                Some(frag) => {
                    let is_last = frag.frag_index + 1 == frag.total_frags;

                    if let Some(assembled) = assembler.add(frag) {
                        let _ = rtk_tx.send(assembled);
                    }

                    // Only log the final fragment so `rx_packet_hex` in the
                    // CSV reflects the packet that completed the batch.
                    if is_last {
                        let _ = rx_log_tx.send(payload.to_vec());
                    }
                }

                None => {
                    log::warn!("[radio] Failed to parse fragment ({} bytes)", payload.len());
                }
            }
        }

        // ── Downlink echo guard ───────────────────────────────────────────────
        // Sirius should never receive its own downlink, but guard anyway.
        DOWNLINK_TYPE => {
            log::debug!("[radio] Ignoring reflected downlink");
        }

        other => {
            log::debug!(
                "[radio] Unknown packet type 0x{:02X} ({} bytes)",
                other,
                payload.len()
            );
        }
    }
}

// ── Debug downlink transmitter ───────────────────────────────────────────────

fn transmit_debug(radio: &mut Rfm95, flight_data: &Arc<Mutex<FlightData>>, seq: u16) {
    let bytes = {
        let data = flight_data.lock().unwrap();
        DebugDownlinkPacket {
            sequence_num: seq,
            gps: data.gps_snr_gps,
            glonass: data.gps_snr_glonass,
            galileo: data.gps_snr_galileo,
            beidou: data.gps_snr_beidou,
            qzss: data.gps_snr_qzss,
        }
        .serialize()
        .to_vec()
    };
    if let Err(e) = radio.transmit(&bytes) {
        log::warn!("[radio] Debug TX error: {}", e);
    }
    if let Err(e) = radio.start_receive_continuous() {
        log::warn!("[radio] RX restart after debug TX: {}", e);
    }
}

fn transmit_downlink(
    radio: &mut Rfm95,
    flight_data: &Arc<Mutex<FlightData>>,
    tx_sequence: &mut u16,
    tx_log_tx: &mpsc::Sender<Vec<u8>>,
    boot: Instant,
) {
    // Build the packet — hold the lock for the minimum possible duration.
    let bytes = {
        let data = flight_data.lock().unwrap();

        let seq = *tx_sequence;
        *tx_sequence = tx_sequence.wrapping_add(1);

        DownlinkPacket {
            sequence_num: seq,
            timestamp_ms: boot.elapsed().as_millis() as u32,
            altitude_m: data.altitude_m,
            velocity_mps: data.velocity_mps,
            accel_z_gs: data.accel_z_gs,
            gps_lat: data.gps_lat,
            gps_lon: data.gps_lon,
            gps_alt_m: data.gps_alt_m,
            rtk_fix: data.rtk_fix,
            pyro_deployed: data.pyro_deployed,
            pyro_continuity: data.pyro_continuity,
            flight_state: data.flight_state.as_u8(),
            gps_snr: data.gps_snr,
        }
        .serialize()
        .to_vec()
    };

    // radio.transmit() handles Standby → Tx → Standby internally.
    match radio.transmit(&bytes) {
        Ok(()) => {
            log::debug!("[radio] TX {} bytes", bytes.len());
            let _ = tx_log_tx.send(bytes);
        }
        Err(e) => {
            log::error!("[radio] TX error: {}", e);
        }
    }

    // Restart continuous RX — transmit() leaves the radio in Standby.
    if let Err(e) = radio.start_receive_continuous() {
        log::error!("[radio] Failed to restart continuous RX after TX: {}", e);
    }
}
