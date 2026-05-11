//! Sopdet ground station — entry point.
//!
//! Starts all subsystems in order:
//! 1. Logger (background CSV writer threads)
//! 2. GPS / RTK (Quectel LC29H via UART — survey-in + RTCM output)
//! 3. RFM95 LoRa radio (SPI + GPIO)
//! 4. HTTP server (tiny_http, Wi-Fi interface)
//!
//! Then the main thread idles, logging a periodic status summary once per
//! minute until the process is killed.

mod gps;
mod logger;
mod radio;
mod server;
mod state;

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Context;
use rfm95::{PinConfig, Rfm95};

use logger::Logger;
use state::AppState;

// ── Hardware configuration ────────────────────────────────────────────────────

/// UART device for the Quectel LC29H GPS module.
const GPS_PORT: &str = "/dev/ttyS0";

/// SPI device for the RFM95W LoRa radio.
const SPI_PATH: &str = "/dev/spidev0.0";

/// Linux GPIO character device used by `gpio-cdev`.
const GPIO_CHIP: &str = "/dev/gpiochip0";

/// RFM95 RESET GPIO pin number (pi-hat schematic: GPIO 17).
const RFM95_RESET_PIN: u32 = 17;

/// RFM95 DIO0 GPIO pin (schematic: GPIO 25).
///
/// DIO0 signals TX_DONE / RX_DONE and is polled by the rfm95 driver.
const RFM95_DIO0_PIN: u32 = 25;

// GPIO 6  → DIO1 (not currently consumed by the rfm95 driver)
// GPIO 26 → DIO2 (not currently consumed by the rfm95 driver)

// ── Server configuration ──────────────────────────────────────────────────────

/// Bind address for the HTTP server.
///
/// Listens on all interfaces so the Android app can reach it over Wi-Fi.
/// Change to `"127.0.0.1:8080"` if you only need local access.
const SERVER_ADDR: &str = "0.0.0.0:8080";

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> anyhow::Result<()> {
    // Initialise env_logger.  Set RUST_LOG=info (or debug/warn) in the
    // environment to control verbosity.
    env_logger::init();

    log::info!("╔══════════════════════════════════════════╗");
    log::info!("║   Sopdet Ground Station — Startup        ║");
    log::info!("╠══════════════════════════════════════════╣");
    log::info!("║  GPS  : {}                      ║", GPS_PORT);
    log::info!("║  Radio: {} (SF7/BW500)     ║", SPI_PATH);
    log::info!("║  HTTP : http://{}              ║", SERVER_ADDR);
    log::info!("╚══════════════════════════════════════════╝");

    // ── Log files ─────────────────────────────────────────────────────────────
    //
    // Each run produces two timestamped CSV files so that logs from separate
    // sessions are never interleaved:
    //   sopdet_telemetry_<unix>.csv  — all transmitted and received packets
    //   sopdet_access_<unix>.csv     — HTTP endpoint accesses
    // Create the logs/ directory if it doesn't already exist.
    std::fs::create_dir_all("logs").context("Cannot create logs/ directory")?;

    let dt = chrono::Local::now().format("%y%m%d_%H%M%S").to_string();
    let telemetry_log = format!("logs/sopdet_{}.csv", dt);
    // Single persistent plain-text log for all HTTP server activity.
    // No date prefix — opened in append mode so all runs share one file.
    let access_log = "logs/sopdet_http.log".to_string();

    log::info!("Telemetry log → {}", telemetry_log);
    log::info!("HTTP log      → {}", access_log);

    let logger = Logger::start(telemetry_log, access_log).context("Cannot start logger threads")?;

    // ── Shared state ──────────────────────────────────────────────────────────
    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::new()));

    // ── GPS / RTK ─────────────────────────────────────────────────────────────
    //
    // setup_and_open opens the UART, starts the reader thread, configures
    // survey-in, saves parameters, and enables RTCM output.  This call blocks
    // while issuing the configuration commands (~2–3 seconds total).
    let gps_opt = match gps::setup_and_open(PathBuf::from(GPS_PORT)) {
        Ok(gps) => {
            log::info!(
                "LC29H GPS ready — survey-in active ({}s / 15.0m)",
                state.lock().map(|s| s.svin_min_duration_s).unwrap_or(150)
            );
            Some(gps)
        }
        Err(e) => {
            log::warn!("Cannot initialise LC29H GPS on '{}': {}", GPS_PORT, e);
            None
        }
    };

    // Channel for raw RTCM bytes flowing from the GPS thread to the radio
    // thread.  The channel is unbounded; RTCM frames are small (≤ a few
    // hundred bytes each) and produced at 1 Hz, so back-pressure is not a
    // concern in practice.
    let (rtcm_tx, rtcm_rx) = mpsc::channel::<Vec<u8>>();

    if let Some(gps) = gps_opt {
        let state = Arc::clone(&state);
        thread::Builder::new()
            .name("gps".to_string())
            .spawn(move || gps::run_gps_thread(gps, rtcm_tx, state))
            .context("Cannot spawn GPS thread")?;
    } else {
        log::warn!("GPS not connected, skipping GPS thread");
    }

    // ── RFM95 LoRa Radio ──────────────────────────────────────────────────────
    let radio_opt = match Rfm95::open(
        SPI_PATH,
        PinConfig {
            gpio_chip: GPIO_CHIP.to_string(),
            reset_pin: RFM95_RESET_PIN,
            dio0_pin: Some(RFM95_DIO0_PIN),
        },
    ) {
        Ok(radio) => {
            log::info!(
                "RFM95 ready — reset=GPIO17 DIO0=GPIO{} DIO1=GPIO6 DIO2=GPIO26",
                RFM95_DIO0_PIN,
            );
            Some(radio)
        }
        Err(e) => {
            log::warn!("Cannot open RFM95 on '{}': {}", SPI_PATH, e);
            None
        }
    };

    if let Some(radio) = radio_opt {
        let state = Arc::clone(&state);
        let radio_logger = logger.clone();
        thread::Builder::new()
            .name("radio".to_string())
            .spawn(move || radio::run_radio_thread(radio, state, rtcm_rx, radio_logger))
            .context("Cannot spawn radio thread")?;
    } else {
        log::warn!("Radio not connected, skipping radio thread");
        drop(rtcm_rx); // Prevent unbounded queue growth if GPS thread is sending RTCM bytes
    }

    // ── HTTP Server ───────────────────────────────────────────────────────────
    {
        let state = Arc::clone(&state);
        let server_addr = SERVER_ADDR.to_string();
        let server_logger = logger.clone();
        thread::Builder::new()
            .name("http-server".to_string())
            .spawn(move || server::run_server(&server_addr, state, server_logger))
            .context("Cannot spawn HTTP server thread")?;
    }

    // ── Main thread — idle loop ───────────────────────────────────────────────
    //
    // The main thread has nothing time-critical to do once all subsystems are
    // up.  It sleeps and periodically logs a status summary so that the
    // operator can confirm everything is healthy without opening the HTTP API.
    log::info!(
        "All subsystems up — base station active on http://{}",
        SERVER_ADDR
    );
    log::info!("Endpoints:");
    log::info!("  GET  http://{SERVER_ADDR}/status");
    log::info!("  GET  http://{SERVER_ADDR}/telemetry/latest");
    log::info!("  GET  http://{SERVER_ADDR}/telemetry/history[?limit=N]");
    log::info!("  POST http://{SERVER_ADDR}/command/emergency");
    log::info!("  POST http://{SERVER_ADDR}/command/deploy");

    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
        log_status_summary(&state);
    }
}

/// Print a one-line status summary to the log once per minute.
fn log_status_summary(state: &Arc<Mutex<AppState>>) {
    if let Ok(s) = state.lock() {
        log::info!(
            "Status — uptime={}s svin_complete={} svin_active={} \
             telemetry_count={} last_rssi={:?}",
            s.uptime_start.elapsed().as_secs(),
            s.svin_complete,
            s.svin_active,
            s.telemetry.len(),
            s.last_downlink_rssi,
        );

        if let Some(gps) = &s.latest_gps {
            log::info!(
                "GPS fix — fix={:?} sats={} lat={:.6} lon={:.6} alt={:.1}m hdop={:.1}",
                gps.fix_quality,
                gps.satellites_used,
                gps.latitude,
                gps.longitude,
                gps.altitude_m,
                gps.hdop,
            );
        }

        log::info!(
            "Base GPS SNR — avg_active={}dBHz  GPS={}  GL={}  GA={}  GB={}",
            s.gps_snr.average_active(),
            s.gps_snr.gps,
            s.gps_snr.glonass,
            s.gps_snr.galileo,
            s.gps_snr.beidou
        );

        if let Some(telem) = s.telemetry.last() {
            log::info!(
                "Last telemetry — seq={} alt={:.1}m vel={:.1}m/s state={} fix={} rssi={}dBm",
                telem.sequence_num,
                telem.altitude_m,
                telem.velocity_mps,
                telem.flight_state,
                telem.rtk_fix,
                telem.rssi,
            );
        }
    }
}
