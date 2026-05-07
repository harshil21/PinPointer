//! Sirius — flight computer entry point.
//!
//! This file is intentionally thin.  All non-trivial logic lives in the
//! dedicated modules listed below.  The main loop runs at the FIRM data
//! rate (~100 Hz) and performs five tasks on every iteration:
//!
//! 1. **FIRM data** — feed new IMU packets to [`DataProcessor`], which
//!    updates [`FlightData`] and advances the flight state machine.
//! 2. **GPS data** — drain the LC29H NMEA stream and update GPS fields in
//!    [`FlightData`].
//! 3. **RTK injection** — forward RTCM correction bytes received from the
//!    radio thread to the LC29H GPS module.
//! 4. **Pyro continuity** — sample GPIO 27 and store the result.
//! 5. **End-of-loop** — check pyro-firing conditions, update buzzer pattern,
//!    log one CSV row.
//!
//! | Module            | Responsibility                                        |
//! |-------------------|-------------------------------------------------------|
//! | [`data_processor`]| [`FlightData`] struct + FIRM packet processing        |
//! | [`state_machine`] | `FlightState` FSM (instant transitions)               |
//! | [`radio`]         | Non-blocking LoRa TX/RX, fragment reassembly          |
//! | [`buzzer`]        | Hardware-PWM buzzer pattern engine (GPIO 18 / PWM0)   |
//! | [`logger`]        | Async CSV logger with `fsync` durability              |
//!
//! # GPIO assignments
//!
//! | GPIO | Direction | Function                                |
//! |------|-----------|-----------------------------------------|
//! |  4   | Output    | RFM95 RESET (managed by `rfm95` crate) |
//! | 18   | PWM0      | Buzzer (managed by `rppal`)            |
//! | 22   | Output    | Pyro ejection charge (fire HIGH 8 ms)  |
//! | 25   | Input     | RFM95 DIO0 (managed by `rfm95` crate)  |
//! | 27   | Input     | Pyro continuity sense                  |

mod buzzer;
mod data_processor;
mod logger;
mod radio;
mod state_machine;

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use time::{OffsetDateTime, UtcOffset};

use anyhow::Context;
use gpio_cdev::{Chip, LineRequestFlags};

use firm_rust::FIRMClient;
use rfm95::{PinConfig, Rfm95};
use rtk::port::BaseGPS;
use rtk::{GpsFixQuality, WireMessage};

use protocol::RtkFixType;

use buzzer::{BuzzerController, BuzzerPattern};
use data_processor::{DataProcessor, FlightData};
use logger::{Logger, build_log_entry};
use radio::run_radio_thread;
use state_machine::FlightState;

// ── Hardware constants ────────────────────────────────────────────────────────

const GPIO_CHIP: &str = "/dev/gpiochip0";
const GPIO_PYRO: u32 = 22;
const GPIO_CONTINUITY: u32 = 27;
// GPIO 18 is managed by rppal/PWM — not requested via gpio-cdev.
const GPIO_RFM95_RESET: u32 = 17;
const GPIO_RFM95_DIO0: u32 = 25;

const FIRM_PORT: &str = "/dev/ttyACM0";
const FIRM_BAUD: u32 = 2_000_000;
const GPS_PORT: &str = "/dev/serial0";
const SPI_PATH: &str = "/dev/spidev0.0";

/// Duration the pyro GPIO pin is held HIGH to fire the ejection charge.
/// Typical e-matches fire within 1–3 ms; 8 ms gives comfortable margin.
const PYRO_PULSE_MS: u64 = 8;

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let boot = Instant::now();

    let dt = OffsetDateTime::now_utc();
    let local = dt.to_offset(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC));

    let fmt =
        time::format_description::parse("[year repr:last_two][month][day]_[hour][minute][second]")
            .unwrap();
    let dt_str = local.format(&fmt).unwrap();
    let log_path = format!("logs/sirius_{}.csv", dt_str);

    log::info!("╔══════════════════════════════════════╗");
    log::info!("║   Sirius Flight Computer — Startup   ║");
    log::info!("╚══════════════════════════════════════╝");
    log::info!("Flight log → {}", log_path);

    // ── GPIO ─────────────────────────────────────────────────────────────────
    let mut chip =
        Chip::new(GPIO_CHIP).with_context(|| format!("Cannot open GPIO chip '{}'", GPIO_CHIP))?;

    // GPIO 22 → pyro ejection charge (output, initially LOW — safe state).
    let pyro_pin = chip
        .get_line(GPIO_PYRO)
        .with_context(|| format!("GPIO {} (pyro) unavailable", GPIO_PYRO))?
        .request(LineRequestFlags::OUTPUT, 0, "sirius-pyro")
        .with_context(|| format!("Cannot claim GPIO {} as output", GPIO_PYRO))?;

    // GPIO 27 → pyro continuity sense (input).
    let continuity_pin = chip
        .get_line(GPIO_CONTINUITY)
        .with_context(|| format!("GPIO {} (continuity) unavailable", GPIO_CONTINUITY))?
        .request(LineRequestFlags::INPUT, 0, "sirius-continuity")
        .with_context(|| format!("Cannot claim GPIO {} as input", GPIO_CONTINUITY))?;

    log::info!(
        "GPIO — pyro=GPIO{} continuity=GPIO{}",
        GPIO_PYRO,
        GPIO_CONTINUITY
    );

    // ── Subsystems ────────────────────────────────────────────────────────────

    let buzzer = BuzzerController::new().context("Failed to initialise hardware PWM buzzer")?;

    let logger =
        Logger::new(&log_path).with_context(|| format!("Cannot open flight log '{}'", log_path))?;

    let mut firm_client = match FIRMClient::new(FIRM_PORT, FIRM_BAUD, 0.1) {
        Ok(mut client) => {
            client.start();
            log::info!("FIRM IMU — {} @ {} baud", FIRM_PORT, FIRM_BAUD);
            Some(client)
        }
        Err(e) => {
            log::warn!("Cannot open FIRM IMU on '{}': {}", FIRM_PORT, e);
            None
        }
    };

    let mut _gps_reader = None;
    let mut gps = match BaseGPS::open_port(PathBuf::from(GPS_PORT)) {
        Ok(mut g) => {
            _gps_reader = Some(g.start());
            log::info!("LC29H GPS — {}", GPS_PORT);
            Some(g)
        }
        Err(e) => {
            log::warn!("Cannot open LC29H GPS on '{}': {}", GPS_PORT, e);
            None
        }
    };

    let radio_opt = match Rfm95::open(
        SPI_PATH,
        PinConfig {
            gpio_chip: GPIO_CHIP.to_string(),
            reset_pin: GPIO_RFM95_RESET,
            dio0_pin: Some(GPIO_RFM95_DIO0),
        },
    ) {
        Ok(r) => {
            log::info!("RFM95 radio — {} (SF7/BW500)", SPI_PATH);
            Some(r)
        }
        Err(e) => {
            log::warn!("Cannot open RFM95 on '{}': {}", SPI_PATH, e);
            None
        }
    };

    // ── Inter-thread channels and atomic flags ────────────────────────────────

    // Main → Radio: latest FlightData snapshot for building downlink packets.
    // The lock is held for only the duration of a clone (~µs).
    let radio_flight_data: Arc<Mutex<FlightData>> = Arc::new(Mutex::new(FlightData::default()));

    // Radio → Main: reassembled RTCM correction bytes.
    let (rtk_tx, rtk_rx) = mpsc::channel::<Vec<u8>>();

    // Radio → Main: raw bytes of the last transmitted / received packets
    // (used solely for the CSV log).
    let (tx_log_tx, tx_log_rx) = mpsc::channel::<Vec<u8>>();
    let (rx_log_tx, rx_log_rx) = mpsc::channel::<Vec<u8>>();

    // Atomic signals from the radio thread to the main thread.
    let emergency_flag = Arc::new(AtomicBool::new(false));
    let deploy_flag = Arc::new(AtomicBool::new(false));
    let contact_lost_flag = Arc::new(AtomicBool::new(false));

    // ── Spawn radio thread ────────────────────────────────────────────────────
    if let Some(radio) = radio_opt {
        let rfd = Arc::clone(&radio_flight_data);
        let ef = Arc::clone(&emergency_flag);
        let df = Arc::clone(&deploy_flag);
        let clf = Arc::clone(&contact_lost_flag);

        thread::Builder::new()
            .name("radio".to_string())
            .spawn(move || {
                run_radio_thread(radio, rfd, rtk_tx, tx_log_tx, rx_log_tx, ef, df, clf, boot)
            })
            .context("Cannot spawn radio thread")?;
    } else {
        log::warn!("Radio not connected, skipping radio thread");
    }

    log::info!("All subsystems up — entering flight loop");

    // ── Flight state ──────────────────────────────────────────────────────────
    let mut processor = DataProcessor::new();
    let mut flight_data = FlightData::default();

    // Most-recently observed TX / RX packet bytes (hex-encoded for the CSV).
    let mut last_tx_hex = String::new();
    let mut last_rx_hex = String::new();

    // ── Main loop ─────────────────────────────────────────────────────────────
    loop {
        // ── 1. FIRM data ─────────────────────────────────────────────────────────────────
        // get_data_packets blocks up to 10 ms waiting for at least one packet,
        // which naturally throttles the loop to the IMU output rate (~100 Hz).
        let mut had_new_data = false;
        let state_changed = if let Some(client) = firm_client.as_mut() {
            match client.get_data_packets(Some(Duration::from_millis(10))) {
                Ok(pkts) if !pkts.is_empty() => {
                    had_new_data = true;
                    processor.update(&pkts, &mut flight_data)
                }
                _ => false,
            }
        } else {
            std::thread::sleep(Duration::from_millis(10));
            false
        };

        // ── 2. GPS data (non-blocking drain) ─────────────────────────────────────
        if let Some(gps_port) = gps.as_mut() {
            while let Some(msg) = gps_port.try_get_gps_data() {
                match msg {
                    WireMessage::NmeaGga(gga) => {
                        log::debug!(
                            "GGA fix={:?} sats={} lat={:.6} lon={:.6}",
                            gga.fix_quality,
                            gga.satellites_used,
                            gga.latitude,
                            gga.longitude
                        );
                        flight_data.gps_lat = gga.latitude;
                        flight_data.gps_lon = gga.longitude;
                        flight_data.gps_alt_m = gga.altitude_m;
                        flight_data.rtk_fix = gga_fix_to_rtk(gga.fix_quality);
                        flight_data.hdop = gga.hdop;
                        flight_data.satellites_used = gga.satellites_used;
                        had_new_data = true;
                    }
                    WireMessage::NmeaGsv(gsv) => {
                        flight_data.gps_snr = gsv.avg_snr();
                        had_new_data = true;
                    }
                    _ => {}
                }
            }
        }

        // ── 3. RTK injection (Radio → GPS module) ─────────────────────────────
        while let Ok(rtk_bytes) = rtk_rx.try_recv() {
            if let Some(gps_port) = gps.as_mut() {
                if let Err(e) = gps_port.write_all(&rtk_bytes) {
                    log::warn!("RTK injection write error: {}", e);
                }
            }
        }

        // ── 4. Pyro continuity ────────────────────────────────────────────────
        flight_data.pyro_continuity = continuity_pin.get_value().map(|v| v == 1).unwrap_or(false);

        // ── 5. Pyro firing ────────────────────────────────────────────────────
        // Fire on:
        //   a) First transition into Freefall (apogee ejection charge).
        //   b) Explicit DeployEjectionCharge command from the ground station.
        // Guard with pyro_deployed so the charge is fired at most once.
        if !flight_data.pyro_deployed {
            let freefall_transition =
                state_changed && flight_data.flight_state == FlightState::Freefall;
            let ground_command = deploy_flag.load(Ordering::Relaxed);

            if freefall_transition || ground_command {
                fire_pyro(&pyro_pin, &mut flight_data, freefall_transition);

                // Acknowledge the ground command so we don't fire again.
                deploy_flag.store(false, Ordering::Relaxed);
            }
        }

        // ── 6. Share latest FlightData with the radio thread ──────────────────
        // This clone is the only lock in the hot path and is held for < 1 µs.
        *radio_flight_data.lock().unwrap() = flight_data.clone();

        // ── 7. Buzzer pattern ─────────────────────────────────────────────────
        let emergency = emergency_flag.load(Ordering::Relaxed);
        let contact_lost = contact_lost_flag.load(Ordering::Relaxed);
        update_buzzer(&flight_data, emergency || contact_lost, &buzzer);

        // ── 8. Drain radio log bytes (non-blocking) ────────────────────────────────────
        if let Ok(tx) = tx_log_rx.try_recv() {
            last_tx_hex = bytes_to_hex(&tx);
            had_new_data = true;
        }
        if let Ok(rx) = rx_log_rx.try_recv() {
            last_rx_hex = bytes_to_hex(&rx);
            had_new_data = true;
        }

        // ── 9. CSV log — only write when new data arrived ─────────────────────────────
        if had_new_data {
            logger.log(build_log_entry(
                &flight_data,
                boot,
                &last_tx_hex,
                &last_rx_hex,
            ));
        }
    }

    // Unreachable in normal operation; lets Drop run on Ctrl-C / SIGTERM.
    // FIRMClient::Drop calls .stop() automatically.
    #[allow(unreachable_code)]
    {
        drop(firm_client);
        Ok(())
    }
}

// ── Pyro channel ──────────────────────────────────────────────────────────────

/// Fire the ejection charge: drive GPIO 22 HIGH for [`PYRO_PULSE_MS`] ms,
/// then LOW.  Marks `data.pyro_deployed = true`.
///
/// Using a timed pulse rather than leaving the pin HIGH is important: it
/// limits energy delivered and protects against software bugs that would
/// otherwise hold the channel on indefinitely.
fn fire_pyro(pin: &gpio_cdev::LineHandle, data: &mut FlightData, freefall_trigger: bool) {
    let reason = if freefall_trigger {
        "freefall"
    } else {
        "ground command"
    };
    log::warn!(
        "*** FIRING PYRO CHANNEL ({}) — alt={:.1} m  vel={:.1} m/s  accel_z={:.2} g ***",
        reason,
        data.altitude_m,
        data.velocity_mps,
        data.accel_z_gs,
    );

    // Drive HIGH — if this fails the situation is already critical.
    pin.set_value(1)
        .expect("CRITICAL: failed to set pyro GPIO 22 high");
    thread::sleep(Duration::from_millis(PYRO_PULSE_MS));
    pin.set_value(0)
        .expect("CRITICAL: failed to set pyro GPIO 22 low");

    data.pyro_deployed = true;
}

// ── Buzzer pattern selection ──────────────────────────────────────────────────

/// Derive the appropriate [`BuzzerPattern`] from the current flight state and
/// update the controller only when the pattern changes (to avoid resetting a
/// mid-sequence on every loop iteration).
fn update_buzzer(data: &FlightData, emergency: bool, buzzer: &BuzzerController) {
    let desired = if emergency {
        // Emergency takes priority over everything — continuous loud tone.
        // log::info!("Emergency signal active — switching buzzer to EMERGENCY pattern");
        BuzzerPattern::Emergency
    } else if data.flight_state == FlightState::Landed {
        // Beep out the apogee altitude so the recovery team can log it.
        // log::info!("Landed — switching buzzer to APOGEE ANNOUNCE pattern ({} m)", data.apogee_m);
        BuzzerPattern::ApogeeAnnounce(data.apogee_m as u32)
    } else if data.flight_state == FlightState::Standby {
        // On the pad: indicate pyro continuity status.
        if data.pyro_continuity {
            // log::info!("Standby — pyro continuity OK");
            BuzzerPattern::StandbyContinuity
        } else {
            // log::info!("Standby — NO pyro continuity");
            BuzzerPattern::StandbyNoContinuity
        }
    } else {
        // Airborne (MotorBurn / Coast / Freefall) — stay silent.
        // log::info!("Airborne (state={:?}) — buzzer silent", data.flight_state);
        BuzzerPattern::Silent
    };

    if buzzer.get_pattern() != desired {
        buzzer.set_pattern(desired);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Map a [`GpsFixQuality`] value from the `rtk` crate to the protocol-level
/// [`RtkFixType`] enum.  Both use the same underlying NMEA GGA quality codes.
fn gga_fix_to_rtk(fix: GpsFixQuality) -> RtkFixType {
    match fix {
        GpsFixQuality::NoFix => RtkFixType::NoFix,
        GpsFixQuality::GpsFix => RtkFixType::GpsFix,
        GpsFixQuality::DgpsFix => RtkFixType::DgpsFix,
        GpsFixQuality::PpsFix => RtkFixType::PpsFix,
        GpsFixQuality::RtkFixed => RtkFixType::RtkFixed,
        GpsFixQuality::RtkFloat => RtkFixType::RtkFloat,
        GpsFixQuality::DeadReckoning => RtkFixType::DeadReckoning,
        GpsFixQuality::Unknown => RtkFixType::NoFix,
    }
}

/// Encode a byte slice as a lower-case hex string for the CSV log.
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
