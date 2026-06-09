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
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use rtk::WireMessage;
use rtk::port::BaseGPS;
use rtk::protocol::commands::{PQTMCfgMsgRate, PQTMCfgNmeaDp, PQTMCfgRcvrMode, PQTMCfgSvin, PQTMMsgName};
use rtk::protocol::pair::{
    NmeaOutputRateTypes, PairCommonSetNmeaOutputRate, PairRTCMSetOutputAntPnt,
    PairRTCMSetOutputMode, RtcmAntPnt, RtcmMode,
};
use rtk::protocol::response::PQTMResponse;

use crate::state::AppState;

// ── Survey-in parameters ──────────────────────────────────────────────────────

/// Survey-in mode byte sent to the LC29H.
const SVIN_MODE: u8 = 1;
/// Maximum allowed 3-D position error for the survey-in (metres).
const SVIN_ACC_LIMIT_M: f32 = 15.0;
/// Default survey-in duration — overridden at runtime by `AppState::svin_min_duration_s`.
const SVIN_DEFAULT_DURATION_S: u32 = 150;

// ── Timing constants ──────────────────────────────────────────────────────────

/// Timeout for individual UART command / response round-trips.
const CMD_TIMEOUT: Duration = Duration::from_secs(5);

/// Sleep between iterations of the GPS polling loop.
/// Short enough to keep the RTCM latency low without burning the CPU.
const GPS_POLL_SLEEP: Duration = Duration::from_millis(100);

/// How long to wait after starting the reader thread before issuing the first
/// command, to give the UART buffer time to prime.
const STARTUP_SETTLE: Duration = Duration::from_millis(800);

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

    configure_gps(&mut gps, SVIN_DEFAULT_DURATION_S);

    Ok(gps)
}

/// Issue all UART configuration commands to the LC29H module.
///
/// Each step is non-fatal: a warning is logged on failure so that a partially
/// configured or pre-configured module still works.
fn configure_gps(gps: &mut BaseGPS, svin_duration_s: u32) {
    // ── 1. Survey-In ─────────────────────────────────────────────────────────
    log::info!(
        "Configuring survey-in: mode={} min_dur={}s acc_limit={:.1}m",
        SVIN_MODE,
        svin_duration_s,
        SVIN_ACC_LIMIT_M,
    );
    let svin_cfg = PQTMCfgSvin {
        mode: SVIN_MODE,
        min_dur: svin_duration_s,
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
    log::info!("Enabling RTCM3 MSM4 output (PAIR432)...");
    match gps.pair_set_rtcm_mode(
        PairRTCMSetOutputMode {
            mode: RtcmMode::Rtcm3Msm4,
        },
        CMD_TIMEOUT,
    ) {
        Ok(_) => log::info!("RTCM3 MSM4 output enabled"),
        Err(e) => log::warn!("RTCM mode set error (non-fatal): {:?}", e),
    }

    // ── 4. Antenna Reference Point — RTCM 1005 (PAIR434) ─────────────────────
    // log::info!("Enabling antenna reference-point output (PAIR434)...");
    // match gps.pair_set_rtcm_antpnt(
    //     PairRTCMSetOutputAntPnt {
    //         ant_pnt: RtcmAntPnt::Enable,
    //     },
    //     CMD_TIMEOUT,
    // ) {
    //     Ok(_) => log::info!("Antenna reference-point output enabled"),
    //     Err(e) => log::warn!("Antenna reference-point set error (non-fatal): {:?}", e),
    // }

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

    // Set base station mode
    // match gps.cfg_rcvrmode_write(PQTMCfgRcvrMode {mode: 2}, CMD_TIMEOUT) {
    //     Ok(_) => log::info!("$PQTMCFGRVCRMODE set"),
    //     Err(e) => log::warn!("Failed to set rcvr mode {:?}", e)
    // }

    // You need to uncomment this for it to truly save it and then power cycle the module
    // match gps.pair_nvram_save_setting(CMD_TIMEOUT) {
    //     Ok(_) => log::info!("Saved to NVM!"),
    //     Err(e) => log::warn!("Failed to save to nvm {:?}", e)
    // }

    // Enable output of NMEA sentences
    for sentence_type in [
        PQTMMsgName::RMC,
        PQTMMsgName::GGA,
        PQTMMsgName::GSV,
        PQTMMsgName::GSA,
        PQTMMsgName::VTG,
    ] {
        match gps.cfg_msgrate_write(
            PQTMCfgMsgRate {
                msg_name: sentence_type.clone(),
                rate: 1,
                msg_ver: 1,
            },
            CMD_TIMEOUT,
        ) {
            Ok(_) => log::info!("${:?} messages enabled", sentence_type),
            Err(e) => log::warn!(
                "{:?} message-rate error (non-fatal): {:?}",
                sentence_type,
                e
            ),
        }
    }

    // Make coordinates more precise (6 decimal places -> 8)
    match gps.cfg_nmea_dp_write(
        PQTMCfgNmeaDp{
            utc_dp: 3,
            pos_dp: 8,
            alt_dp: 3,
            dop_dp: 2,
            spd_dp: 3,
            cog_dp: 2,
        }, CMD_TIMEOUT) {
            Ok(_) => log::info!("Increased decimal precision for coords"),
            Err(e) => log::warn!(
                "Failed to increase decimal precision: {:?}",
                e
            ),
        }

    log::info!("Saving parameters to flash ($PQTMSAVEPAR)...");
    match gps.save_par(CMD_TIMEOUT) {
        Ok(_) => log::info!("Parameters saved to flash OK"),
        Err(e) => log::warn!("Save-parameters error (non-fatal): {:?}", e),
    }

    match gps.pair_nvram_save_setting(CMD_TIMEOUT) {
        Ok(_) => log::info!("Saved to NVM!"),
        Err(e) => log::warn!("Failed to save to nvm {:?}", e),
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

    // Optimistically mark survey-in as active so the UI shows a meaningful
    // state before the first $PQTMSVINSTATUS message arrives.
    if let Ok(mut s) = state.lock() {
        if !s.svin_complete {
            s.svin_active = true;
        }
    }

    let mut snr_log_ctx = SnrLogCtx {
        last_log: Instant::now()
            .checked_sub(Duration::from_secs(2))
            .unwrap_or_else(Instant::now),
        dirty: false,
    };

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
            let duration_s = state.lock().map(|s| s.svin_min_duration_s).unwrap_or(150);
            // TODO: Fix this: We should ideally restart the gnss engine (PQTMGNSSSTOP & PQTMGNSSSTART)
            // configure_gps(&mut gps, duration_s);
            log::info!(
                "Resurvey configuration sent to GPS module ({}s)",
                duration_s
            );
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
            handle_wire_message(wire, &state, &mut snr_log_ctx);
        }

        // Emit a consolidated per-constellation SNR info line once per second.
        if snr_log_ctx.dirty && snr_log_ctx.last_log.elapsed() >= Duration::from_secs(1) {
            if let Ok(s) = state.lock() {
                log::info!(
                    "Base GPS SNR (dB-Hz) — avg_active={} GPS={} GL={} GA={} GB={} GQ={}",
                    s.gps_snr.average_active(),
                    s.gps_snr.gps,
                    s.gps_snr.glonass,
                    s.gps_snr.galileo,
                    s.gps_snr.beidou,
                    s.gps_snr.qzss,
                );
            }
            snr_log_ctx.last_log = Instant::now();
            snr_log_ctx.dirty = false;
        }

        std::thread::sleep(GPS_POLL_SLEEP);
    }
}

/// Tracking state for the 1 Hz consolidated GPS-SNR info log.
struct SnrLogCtx {
    last_log: Instant,
    /// Set whenever a GSV update lands; cleared on the next emitted line.
    dirty: bool,
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn handle_wire_message(
    wire: WireMessage,
    state: &Arc<Mutex<AppState>>,
    snr_log_ctx: &mut SnrLogCtx,
) {
    match wire {
        // GPS fix data from NMEA GGA sentences.
        WireMessage::NmeaGga(gga) => {
            log::debug!(
                "GGA fix={:?} sats={} lat={:.8} lon={:.8} alt={:.1}m hdop={:.1}",
                gga.fix_quality,
                gga.satellites_used,
                gga.latitude,
                gga.longitude,
                gga.altitude_m,
                gga.hdop,
            );
            if let Ok(mut s) = state.lock() {
                // Don't let a transient "NoFix" GGA (lat=0, lon=0) wipe out a
                // previously valid fix.  The LC29H occasionally emits a
                // zero-coord GGA during constellation re-acquisition or
                // momentary loss of lock; the Android UI would otherwise
                // briefly show "No GPS Fix" or flicker to 0°/0° and only
                // recover on the next 1 Hz GGA.  Keep the last known good
                // fix until a new GGA with non-zero coords arrives.
                let is_zero = gga.latitude == 0.0 && gga.longitude == 0.0;
                let had_valid = s
                    .latest_gps
                    .as_ref()
                    .map(|g| g.latitude != 0.0 || g.longitude != 0.0)
                    .unwrap_or(false);
                if !(is_zero && had_valid) {
                    s.latest_gps = Some(gga);
                }
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
                // Always record live metrics for display.
                s.svin_accuracy_m = status.mean_acc;
                s.svin_observations = status.observations;
                s.svin_elapsed_s = status.config_duration;

                match status.valid {
                    1 => {
                        s.svin_active = true;
                        s.svin_complete = false;
                    }
                    2 => {
                        if !s.svin_complete {
                            log::info!(
                                "Survey-in COMPLETE — ECEF ({:.3}, {:.3}, {:.3}) \
                                 acc={:.2}m after {}s",
                                status.mean_x,
                                status.mean_y,
                                status.mean_z,
                                status.mean_acc,
                                status.config_duration
                            );
                        }
                        s.svin_complete = true;
                        s.svin_active = false;
                    }
                    _ => {
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
        // Average SNR update from satellite view sentences. The consolidated
        // info-level summary is emitted once per second in the polling loop
        // (see SnrLogCtx); per-GSV detail stays at debug to avoid spam.
        WireMessage::NmeaGsv(gsv) => {
            let avg = gsv.avg_snr();
            let cname = gsv.constellation.as_str();
            let total = gsv.satellites.len();
            let with_snr = gsv.satellites.iter().filter(|s| s.snr.is_some()).count();

            log::debug!(
                "GSV: {}/{} {} satellites have SNR, avg = {} dB-Hz",
                with_snr,
                total,
                cname,
                avg
            );

            // Only update when there are tracked satellites with valid SNR.
            // This prevents zero-satellite sequences (e.g. $GQGSV,1,1,00)
            // from zeroing out a previously valid reading.
            if avg > 0 {
                if let Ok(mut s) = state.lock() {
                    use rtk::GsvConstellation::*;
                    match gsv.constellation {
                        Gps => s.gps_snr.gps = avg,
                        Glonass => s.gps_snr.glonass = avg,
                        Galileo => s.gps_snr.galileo = avg,
                        BeiDou => s.gps_snr.beidou = avg,
                        Qzss => s.gps_snr.qzss = avg,
                        NavIc => s.gps_snr.navic = avg,
                        _ => {} // GN and Unknown: skip
                    }
                }
                snr_log_ctx.dirty = true;
            } else {
                log::debug!("GSV: {} has no tracked satellites — skipping update", cname);
            }
        }

        // All other PQTM / PAIR messages are ignored by the polling loop
        // (command responses are consumed synchronously during setup).
        _ => {}
    }
}
