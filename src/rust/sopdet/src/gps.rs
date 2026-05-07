//! GPS / RTK subsystem for the sopdet ground station.
//!
//! This module handles:
//! 1. Opening the Quectel LC29H UART port.
//! 2. Configuring survey-in via `$PQTMCFGSVIN` (1 minute, 15 m 3D accuracy).
//! 3. Saving parameters to flash via `$PQTMSAVEPAR`.
//! 4. Enabling RTCM3 MSM7 correction output via `PAIR432`.
//! 5. Enabling the antenna reference-point message (RTCM 1005) via `PAIR434`.
//! 6. Enabling `$PQTMSVINSTATUS` messages for survey-in progress monitoring.
//! 7. Running a polling loop that forwards RTCM frames to the radio thread and
//!    updates [`AppState`] with the latest GPS fix and survey-in status.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use rtk::WireMessage;
use rtk::port::BaseGPS;
use rtk::protocol::commands::{PQTMCfgMsgRate, PQTMCfgSvin, PQTMMsgName};
use rtk::protocol::pair::{PairRTCMSetOutputAntPnt, PairRTCMSetOutputMode, RtcmAntPnt, RtcmMode};
use rtk::protocol::response::PQTMResponse;

use crate::state::AppState;

// ── Survey-in parameters ──────────────────────────────────────────────────────

/// Survey-in mode byte sent to the LC29H.
/// 0 = disabled, 1 = survey-in, 2 = fixed position.
const SVIN_MODE: u8 = 1;

/// Minimum observation time the module must accumulate before concluding
/// the survey-in (seconds).  1 minute as requested.
const SVIN_MIN_DURATION_S: u32 = 60;

/// Maximum allowed mean 3D position error for the survey-in to be accepted
/// (metres).
const SVIN_ACC_LIMIT_M: f32 = 15.0;

// ── Timing constants ──────────────────────────────────────────────────────────

/// Timeout for individual UART command / response round-trips.
const CMD_TIMEOUT: Duration = Duration::from_secs(10);

/// Sleep between iterations of the GPS polling loop.
/// Short enough to keep the RTCM latency low without burning the CPU.
const GPS_POLL_SLEEP: Duration = Duration::from_millis(100);

/// How long to wait after starting the reader thread before issuing the first
/// command, to give the UART buffer time to prime.
const STARTUP_SETTLE: Duration = Duration::from_millis(500);

// ── Public API ────────────────────────────────────────────────────────────────

/// Open and fully configure the Quectel LC29H for RTK base-station operation.
///
/// This call blocks while issuing configuration commands over UART.  All
/// command errors are logged as warnings rather than propagated — the GPS
/// module may have been pre-configured from a previous run, so a rejected
/// command is not necessarily fatal.
///
/// # Errors
///
/// Returns an error only if the serial port itself cannot be opened.
pub fn setup_and_open(port_path: PathBuf) -> Result<BaseGPS> {
    log::info!("Opening GPS UART: {}", port_path.display());

    let mut gps = BaseGPS::open_port(port_path.clone())
        .with_context(|| format!("Cannot open GPS port '{}'", port_path.display()))?;

    log::info!("Starting GPS reader thread...");
    let _reader = gps.start();

    // Allow the reader thread and UART to stabilise before sending commands.
    std::thread::sleep(STARTUP_SETTLE);

    configure_gps(&mut gps);

    Ok(gps)
}

/// Issue all UART configuration commands to the LC29H module.
///
/// Each step is non-fatal: a warning is logged on failure so that a partially
/// configured or pre-configured module still works.
fn configure_gps(gps: &mut BaseGPS) {
    // ── 1. Survey-In ─────────────────────────────────────────────────────────
    log::info!(
        "Configuring survey-in: mode={} min_dur={}s acc_limit={:.1}m",
        SVIN_MODE,
        SVIN_MIN_DURATION_S,
        SVIN_ACC_LIMIT_M,
    );
    let svin_cfg = PQTMCfgSvin {
        mode: SVIN_MODE,
        min_dur: SVIN_MIN_DURATION_S,
        acc_limit_m: SVIN_ACC_LIMIT_M,
        // ECEF coordinates are unused in survey-in mode (module determines
        // its own position).
        ecef_x: 0.0,
        ecef_y: 0.0,
        ecef_z: 0.0,
    };
    match gps.cfg_svin_write(svin_cfg, CMD_TIMEOUT) {
        Ok(_) => log::info!("Survey-in configured OK"),
        Err(e) => log::warn!("Survey-in config error (non-fatal): {:?}", e),
    }

    // ── 2. Save Parameters to Flash ───────────────────────────────────────────
    log::info!("Saving parameters to flash ($PQTMSAVEPAR)...");
    match gps.save_par(CMD_TIMEOUT) {
        Ok(_) => log::info!("Parameters saved to flash OK"),
        Err(e) => log::warn!("Save-parameters error (non-fatal): {:?}", e),
    }

    // ── 3. RTCM3 MSM7 Output (PAIR432) ───────────────────────────────────────
    log::info!("Enabling RTCM3 MSM7 output (PAIR432)...");
    match gps.pair_set_rtcm_mode(
        PairRTCMSetOutputMode {
            mode: RtcmMode::Rtcm3Msm7,
        },
        CMD_TIMEOUT,
    ) {
        Ok(_) => log::info!("RTCM3 MSM7 output enabled"),
        Err(e) => log::warn!("RTCM mode set error (non-fatal): {:?}", e),
    }

    // ── 4. Antenna Reference Point — RTCM 1005 (PAIR434) ─────────────────────
    log::info!("Enabling antenna reference-point output (PAIR434)...");
    match gps.pair_set_rtcm_antpnt(
        PairRTCMSetOutputAntPnt {
            ant_pnt: RtcmAntPnt::Enable,
        },
        CMD_TIMEOUT,
    ) {
        Ok(_) => log::info!("Antenna reference-point output enabled"),
        Err(e) => log::warn!("Antenna reference-point set error (non-fatal): {:?}", e),
    }

    // ── 5. Enable $PQTMSVINSTATUS Messages ───────────────────────────────────
    // Rate=1 means one message per second, msg_ver=1 as per LC29H datasheet.
    log::info!("Enabling $PQTMSVINSTATUS messages at 1 Hz...");
    match gps.cfg_msgrate_write(
        PQTMCfgMsgRate {
            msg_name: PQTMMsgName::SvinStatus,
            rate: 1,
            msg_ver: 1,
        },
        CMD_TIMEOUT,
    ) {
        Ok(_) => log::info!("$PQTMSVINSTATUS messages enabled"),
        Err(e) => log::warn!("SVIN status message-rate error (non-fatal): {:?}", e),
    }
}

// ── GPS polling thread ────────────────────────────────────────────────────────

/// GPS polling loop — runs in a dedicated thread (blocks forever).
///
/// Responsibilities:
/// - Drain RTCM correction frames from the LC29H and forward them to the
///   radio thread via `rtcm_tx`.
/// - Drain NMEA GGA sentences and update [`AppState::latest_gps`].
/// - Monitor `$PQTMSVINSTATUS` messages and update
///   [`AppState::svin_complete`] / [`AppState::svin_active`].
pub fn run_gps_thread(
    mut gps: BaseGPS,
    rtcm_tx: mpsc::Sender<Vec<u8>>,
    state: Arc<Mutex<AppState>>,
) {
    log::info!("GPS polling thread started");

    loop {
        // ── Resurvey check ────────────────────────────────────────────────────
        // The HTTP server sets resurvey_requested when the operator calls
        // POST /resurvey (e.g. after moving the base station).  We re-run the
        // full GPS configuration to restart survey-in from scratch.
        let do_resurvey = if let Ok(mut s) = state.lock() {
            if s.resurvey_requested {
                s.resurvey_requested = false;
                s.svin_complete = false;
                s.svin_active = false;
                true
            } else {
                false
            }
        } else {
            false
        };

        if do_resurvey {
            log::info!("Resurvey requested — restarting GPS survey-in...");
            configure_gps(&mut gps);
            log::info!("Resurvey configuration sent to GPS module");
        }

        // ── RTCM correction frames ────────────────────────────────────────────
        while let Some(msg) = gps.try_get_rtcm_data() {
            log::debug!("RTCM type={} len={}", msg.message_type, msg.raw_data.len());
            if rtcm_tx.send(msg.raw_data).is_err() {
                log::error!("RTCM channel closed — GPS thread exiting");
                return;
            }
        }

        // ── NMEA / PQTM messages ─────────────────────────────────────────────
        while let Some(wire) = gps.try_get_gps_data() {
            handle_wire_message(wire, &state);
        }

        std::thread::sleep(GPS_POLL_SLEEP);
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn handle_wire_message(wire: WireMessage, state: &Arc<Mutex<AppState>>) {
    match wire {
        // GPS fix data from NMEA GGA sentences.
        WireMessage::NmeaGga(gga) => {
            log::debug!(
                "GGA fix={:?} sats={} lat={:.6} lon={:.6} alt={:.1}m hdop={:.1}",
                gga.fix_quality,
                gga.satellites_used,
                gga.latitude,
                gga.longitude,
                gga.altitude_m,
                gga.hdop,
            );
            if let Ok(mut s) = state.lock() {
                s.latest_gps = Some(gga);
            }
        }

        // Survey-in progress / completion status.
        WireMessage::PQTMMessage(PQTMResponse::SvinStatus(status)) => {
            log::info!(
                "SVIN status: valid={} obs={} elapsed={}s acc={:.2}m \
                 mean_ecef=({:.3},{:.3},{:.3})",
                status.valid,
                status.observations,
                status.config_duration,
                status.mean_acc,
                status.mean_x,
                status.mean_y,
                status.mean_z,
            );

            if let Ok(mut s) = state.lock() {
                match status.valid {
                    1 => {
                        // In progress.
                        s.svin_active = true;
                        s.svin_complete = false;
                    }
                    2 => {
                        // Converged.
                        if !s.svin_complete {
                            log::info!(
                                "Survey-in COMPLETE — ECEF ({:.3}, {:.3}, {:.3}) \
                                 acc={:.2}m after {}s",
                                status.mean_x,
                                status.mean_y,
                                status.mean_z,
                                status.mean_acc,
                                status.config_duration,
                            );
                        }
                        s.svin_complete = true;
                        s.svin_active = false;
                    }
                    _ => {
                        // Not started or invalid.
                        s.svin_active = false;
                    }
                }
            }
        }

        // EPE (estimated position error) — log at debug level only.
        WireMessage::PQTMMessage(PQTMResponse::Epe(epe)) => {
            log::debug!(
                "EPE: N={:.2}m E={:.2}m D={:.2}m 2D={:.2}m 3D={:.2}m",
                epe.epe_north,
                epe.epe_east,
                epe.epe_down,
                epe.epe_2d,
                epe.epe_3d,
            );
        }

        // All other PQTM / PAIR messages are ignored by the polling loop
        // (command responses are consumed synchronously during setup).
        // Average SNR update from satellite view sentences.
        WireMessage::NmeaGsv(gsv) => {
            let avg = gsv.avg_snr();
            log::debug!(
                "GSV: {} satellites, avg SNR = {} dB-Hz",
                gsv.satellites.len(),
                avg
            );
            if let Ok(mut s) = state.lock() {
                s.gps_snr = avg;
            }
        }

        // All other PQTM / PAIR messages are ignored by the polling loop
        // (command responses are consumed synchronously during setup).
        _ => {}
    }
}
