//! LoRa radio management for the sopdet ground station.
//!
//! This module runs the RFM95 radio in a dedicated thread.  It:
//! - Continuously receives [`DownlinkPacket`] telemetry from the rocket.
//! - Forwards received packets to [`AppState`] and the telemetry CSV log.
//! - Drains RTCM correction bytes from the GPS thread and transmits them
//!   toward the rocket as either a single [`UplinkPacket`] (≤ 252 bytes) or
//!   a sequence of [`RtcmFragment`] packets (> 252 bytes).
//! - Bundles any pending [`GroundCommand`] from the HTTP server with the next
//!   outgoing RTCM uplink, or sends it immediately as a standalone uplink when
//!   no RTCM data is pending.
//!
//! The LoRa configuration mirrors the sirius rocket: **SF7 / BW500 / CR4-5 /
//! 915 MHz** for the highest data rate compatible with the hardware.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use protocol::{
    DOWNLINK_TYPE, DownlinkPacket, GroundCommand, MAX_FRAG_DATA, MAX_UPLINK_RTK, RtcmFragment,
    UplinkPacket,
};
use rfm95::{Bandwidth, LoraConfig, Rfm95, SpreadingFactor};

use crate::logger::{Logger, TelemetryLogEntry};
use crate::state::{AppState, TelemetryEntry};

// ── Timing constants ──────────────────────────────────────────────────────────

/// Maximum time to wait for a single LoRa transmission to complete.
const TX_TIMEOUT: Duration = Duration::from_secs(5);

/// CPU yield between poll iterations.
const POLL_SLEEP: Duration = Duration::from_millis(1);

// ── LoRa configuration ────────────────────────────────────────────────────────

/// SF7 / BW500 — matches the sirius rocket configuration exactly.
///
/// This gives the highest data rate on the RFM95 which minimises time-on-air
/// for RTCM correction frames, reducing the chance of colliding with a
/// downlink from the rocket.
fn radio_config() -> LoraConfig {
    LoraConfig {
        spreading_factor: SpreadingFactor::Sf7,
        bandwidth: Bandwidth::Bw500kHz,
        ..LoraConfig::default()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Return milliseconds since the Unix epoch.
fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Radio management loop — blocks forever, call from a dedicated thread.
///
/// # Arguments
///
/// * `radio`    — Opened (but not yet configured) [`Rfm95`] instance.
/// * `state`    — Shared application state (telemetry ring buffer, pending
///                commands, GPS fix, survey-in status).
/// * `rtcm_rx`  — Receives raw RTCM correction bytes from the GPS thread.
/// * `logger`   — Cloneable handle to the background CSV logger.
pub fn run_radio_thread(
    mut radio: Rfm95,
    state: Arc<Mutex<AppState>>,
    rtcm_rx: mpsc::Receiver<Vec<u8>>,
    logger: Logger,
) {
    log::info!("Configuring RFM95 (SF7 / BW500 / 915 MHz)...");
    if let Err(e) = radio.configure(&radio_config()) {
        log::error!("RFM95 configure failed: {}", e);
        return;
    }

    log::info!("Starting continuous receive...");
    if let Err(e) = radio.start_receive_continuous() {
        log::error!("Cannot start continuous receive: {}", e);
        return;
    }

    // Incrementing session identifier for RTCM fragmentation.
    // Wraps at 255 → 0, matching the u8 field in RtcmFragment.
    let mut session_id: u8 = 0;

    log::info!("Radio thread active — listening for downlinks from rocket");

    loop {
        // ── 1. Poll for an incoming packet (non-blocking) ─────────────────────
        match radio.poll_receive() {
            Ok(Some(pkt)) if !pkt.payload.is_empty() => {
                if pkt.payload[0] == DOWNLINK_TYPE {
                    match DownlinkPacket::deserialize(&pkt.payload) {
                        Some(dl) => handle_downlink(dl, pkt.rssi, pkt.snr, &state, &logger),
                        None => log::warn!(
                            "Received DOWNLINK_TYPE packet but deserialisation failed \
                             (len={})",
                            pkt.payload.len()
                        ),
                    }
                } else {
                    // Silently ignore packets we didn't send (e.g. stray uplinks
                    // echoed back, or foreign LoRa traffic on the same frequency).
                    log::debug!(
                        "Ignoring packet with unknown type byte 0x{:02x} (len={})",
                        pkt.payload[0],
                        pkt.payload.len()
                    );
                }
                // Always restart RX after the radio exits receive mode.
                restart_rx(&mut radio);
            }
            Ok(Some(_)) => {
                // Empty payload — restart and continue.
                restart_rx(&mut radio);
            }
            Ok(None) => {
                // No packet received this tick; keep polling.
            }
            Err(e) => {
                log::error!("poll_receive error: {}", e);
                restart_rx(&mut radio);
            }
        }

        // ── 2. Check for RTCM data from the GPS thread ────────────────────────
        //
        // Bundle any pending command with the RTCM uplink so the rocket gets
        // both in a single transmission.  If there is no pending command,
        // GroundCommand::None is used (meaning "RTK data only").
        if let Ok(rtcm_bytes) = rtcm_rx.try_recv() {
            let command = pop_pending_command(&state).unwrap_or(GroundCommand::None);
            transmit_rtcm(&mut radio, rtcm_bytes, command, &mut session_id, &logger);
            restart_rx(&mut radio);
        } else if let Some(command) = pop_pending_command(&state) {
            // No RTCM data this tick but the operator issued a command — send
            // it immediately rather than waiting for the next RTCM frame.
            transmit_command_only(&mut radio, command, &logger);
            restart_rx(&mut radio);
        }

        std::thread::sleep(POLL_SLEEP);
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Restart the radio in continuous-receive mode, logging any error.
fn restart_rx(radio: &mut Rfm95) {
    if let Err(e) = radio.start_receive_continuous() {
        log::error!("Cannot restart continuous receive: {}", e);
    }
}

/// Pop the front of the pending-commands queue, returning `None` if empty.
fn pop_pending_command(state: &Arc<Mutex<AppState>>) -> Option<GroundCommand> {
    state
        .lock()
        .ok()
        .and_then(|mut s| s.pending_commands.pop_front())
}

// ── Downlink handling ─────────────────────────────────────────────────────────

/// Process a successfully deserialised [`DownlinkPacket`].
///
/// Stores the entry in the shared [`AppState`] telemetry ring buffer and
/// enqueues a row in the telemetry CSV log.
fn handle_downlink(
    dl: DownlinkPacket,
    rssi: i16,
    snr: f32,
    state: &Arc<Mutex<AppState>>,
    logger: &Logger,
) {
    let ts = unix_ms();
    let rtk_fix_str = dl.rtk_fix.as_str().to_string();

    log::info!(
        "RX DOWNLINK seq={} alt={:.1}m vel={:.1}m/s state={} fix={} \
         pyro_dep={} rssi={}dBm snr={:.1}dB rocket_snr={}dBHz",
        dl.sequence_num,
        dl.altitude_m,
        dl.velocity_mps,
        dl.flight_state,
        rtk_fix_str,
        dl.pyro_deployed,
        rssi,
        snr,
        dl.gps_snr,
    );

    // ── Update shared state ───────────────────────────────────────────────────
    let entry = TelemetryEntry {
        received_at: ts,
        sequence_num: dl.sequence_num,
        timestamp_ms: dl.timestamp_ms,
        altitude_m: dl.altitude_m,
        velocity_mps: dl.velocity_mps,
        accel_z_gs: dl.accel_z_gs,
        gps_lat: dl.gps_lat,
        gps_lon: dl.gps_lon,
        gps_alt_m: dl.gps_alt_m,
        rtk_fix: rtk_fix_str.clone(),
        pyro_deployed: dl.pyro_deployed,
        pyro_continuity: dl.pyro_continuity,
        flight_state: dl.flight_state,
        rssi,
        snr,
        gps_snr: dl.gps_snr,
    };

    let base_snr = if let Ok(mut s) = state.lock() {
        s.last_downlink_rssi = Some(rssi);
        s.add_telemetry(entry);
        s.gps_snr
    } else {
        0
    };

    // ── Telemetry CSV log ─────────────────────────────────────────────────────
    logger.log_telemetry(TelemetryLogEntry {
        timestamp_ms: ts,
        direction: "RX".to_string(),
        sequence_num: Some(dl.sequence_num),
        altitude_m: Some(dl.altitude_m),
        velocity_mps: Some(dl.velocity_mps),
        accel_z_gs: Some(dl.accel_z_gs),
        gps_lat: Some(dl.gps_lat),
        gps_lon: Some(dl.gps_lon),
        gps_alt_m: Some(dl.gps_alt_m),
        rtk_fix: Some(rtk_fix_str),
        pyro_deployed: Some(dl.pyro_deployed),
        pyro_continuity: Some(dl.pyro_continuity),
        flight_state: Some(dl.flight_state),
        rssi: Some(rssi),
        snr: Some(snr),
        rocket_gps_snr: Some(dl.gps_snr),
        base_gps_snr: Some(base_snr),
        command: None,
        rtk_data_len: None,
        fragment_session: None,
        fragment_index: None,
        fragment_total: None,
    });
}

// ── Uplink / fragment transmission ───────────────────────────────────────────

/// Transmit RTCM correction data (and optionally a command) to the rocket.
///
/// Strategy:
/// - If `data.len()` ≤ [`MAX_UPLINK_RTK`] (252 bytes), pack everything into
///   a single [`UplinkPacket`] and send.
/// - Otherwise, split the data into [`RtcmFragment`] packets (≤ 250 bytes
///   each) and send them in sequence.  If a command was also pending, send it
///   afterward as a standalone [`UplinkPacket`] so it is never lost.
///
/// Each fragment session uses the current `session_id`, which is then
/// incremented (wrapping at 255) to distinguish the next session.
fn transmit_rtcm(
    radio: &mut Rfm95,
    data: Vec<u8>,
    command: GroundCommand,
    session_id: &mut u8,
    logger: &Logger,
) {
    if data.len() <= MAX_UPLINK_RTK {
        // ── Single UplinkPacket ───────────────────────────────────────────────
        let uplink = UplinkPacket {
            command,
            rtk_data: data.clone(),
        };
        let bytes = uplink.serialize();

        log::info!("TX UPLINK cmd={} rtk_len={}", command, data.len());

        match radio.transmit_with_timeout(&bytes, TX_TIMEOUT) {
            Ok(_) => {
                logger.log_telemetry(TelemetryLogEntry {
                    timestamp_ms: unix_ms(),
                    direction: "TX".to_string(),
                    sequence_num: None,
                    altitude_m: None,
                    velocity_mps: None,
                    accel_z_gs: None,
                    gps_lat: None,
                    gps_lon: None,
                    gps_alt_m: None,
                    rtk_fix: None,
                    pyro_deployed: None,
                    pyro_continuity: None,
                    flight_state: None,
                    rssi: None,
                    snr: None,
                    rocket_gps_snr: None,
                    base_gps_snr: None,
                    command: Some(command.to_string()),
                    rtk_data_len: Some(data.len()),
                    fragment_session: None,
                    fragment_index: None,
                    fragment_total: None,
                });
            }
            Err(e) => log::error!("TX UPLINK error: {}", e),
        }
    } else {
        // ── Fragmented RtcmFragment packets ───────────────────────────────────
        let sid = *session_id;
        *session_id = session_id.wrapping_add(1);

        let chunks: Vec<&[u8]> = data.chunks(MAX_FRAG_DATA).collect();
        let total = chunks.len() as u8;

        log::info!(
            "TX FRAGMENT session={} total_frags={} total_bytes={}",
            sid,
            total,
            data.len()
        );

        let mut all_ok = true;
        for (i, chunk) in chunks.iter().enumerate() {
            let frag = RtcmFragment {
                session_id: sid,
                frag_index: i as u8,
                total_frags: total,
                data: chunk.to_vec(),
            };
            let bytes = frag.serialize();

            match radio.transmit_with_timeout(&bytes, TX_TIMEOUT) {
                Ok(_) => {
                    logger.log_telemetry(TelemetryLogEntry {
                        timestamp_ms: unix_ms(),
                        direction: "TX".to_string(),
                        sequence_num: None,
                        altitude_m: None,
                        velocity_mps: None,
                        accel_z_gs: None,
                        gps_lat: None,
                        gps_lon: None,
                        gps_alt_m: None,
                        rtk_fix: None,
                        pyro_deployed: None,
                        pyro_continuity: None,
                        flight_state: None,
                        rssi: None,
                        snr: None,
                        rocket_gps_snr: None,
                        base_gps_snr: None,
                        command: None,
                        rtk_data_len: Some(chunk.len()),
                        fragment_session: Some(sid),
                        fragment_index: Some(i as u8),
                        fragment_total: Some(total),
                    });
                    // Restart RX between fragment transmissions so we don't
                    // miss a downlink during a multi-fragment burst.
                    if let Err(e) = radio.start_receive_continuous() {
                        log::warn!("Cannot restart RX between fragments: {}", e);
                    }
                }
                Err(e) => {
                    log::error!(
                        "TX FRAGMENT error (session={} frag={}/{}): {}",
                        sid,
                        i + 1,
                        total,
                        e
                    );
                    all_ok = false;
                    break; // Abort — the rover will time-out the incomplete session.
                }
            }
        }

        if all_ok {
            log::info!(
                "TX FRAGMENT session={} complete ({} frags, {} bytes)",
                sid,
                total,
                data.len()
            );
        }

        // If the operator also issued a command, send it now as a standalone
        // uplink so it is not silently dropped.
        if command != GroundCommand::None {
            transmit_command_only(radio, command, logger);
        }
    }
}

/// Send a standalone [`UplinkPacket`] carrying only a command (no RTK data).
///
/// Used when there is a pending command but no RTCM frame ready to bundle it
/// with, or after a fragmented session when the command could not be inlined.
fn transmit_command_only(radio: &mut Rfm95, command: GroundCommand, logger: &Logger) {
    let uplink = UplinkPacket {
        command,
        rtk_data: vec![],
    };
    let bytes = uplink.serialize();

    log::info!("TX COMMAND: {}", command);

    match radio.transmit_with_timeout(&bytes, TX_TIMEOUT) {
        Ok(_) => {
            logger.log_telemetry(TelemetryLogEntry {
                timestamp_ms: unix_ms(),
                direction: "TX".to_string(),
                sequence_num: None,
                altitude_m: None,
                velocity_mps: None,
                accel_z_gs: None,
                gps_lat: None,
                gps_lon: None,
                gps_alt_m: None,
                rtk_fix: None,
                pyro_deployed: None,
                pyro_continuity: None,
                flight_state: None,
                rssi: None,
                snr: None,
                rocket_gps_snr: None,
                base_gps_snr: None,
                command: Some(command.to_string()),
                rtk_data_len: Some(0),
                fragment_session: None,
                fragment_index: None,
                fragment_total: None,
            });
        }
        Err(e) => log::error!("TX COMMAND error ({}): {}", command, e),
    }
}
