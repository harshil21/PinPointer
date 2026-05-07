//! Background loggers for telemetry events and HTTP access records.
//!
//! Two log files are written:
//! - **Telemetry log** — CSV, one row per transmitted or received radio packet.
//! - **HTTP access log** — plain text, one timestamped line per HTTP request,
//!   appended across runs so the full session history is always in one file.
//!
//! Each logger runs a dedicated background thread that owns the file writer,
//! keeping the hot paths (radio thread, server thread) non-blocking.  The
//! [`Logger`] handle is cheaply [`Clone`]able because it is backed by
//! `mpsc::Sender` channels.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::Serialize;

/// Returns milliseconds since the Unix epoch.
pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Log entry types ───────────────────────────────────────────────────────────

/// One row in the telemetry CSV log.
///
/// Fields that are not applicable for a given entry are `None` and are
/// written as empty cells in the CSV.
#[derive(Serialize, Debug, Clone)]
pub struct TelemetryLogEntry {
    /// Milliseconds since the Unix epoch.
    pub timestamp_ms: u64,
    /// `"RX"` for downlinks received from the rocket; `"TX"` for uplinks /
    /// fragments sent toward the rocket.
    pub direction: String,

    // ── Downlink fields (RX only) ─────────────────────────────────────────────
    pub sequence_num: Option<u16>,
    pub altitude_m: Option<f32>,
    pub velocity_mps: Option<f32>,
    pub accel_z_gs: Option<f32>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub gps_alt_m: Option<f32>,
    pub rtk_fix: Option<String>,
    pub pyro_deployed: Option<bool>,
    pub pyro_continuity: Option<bool>,
    /// 0 = Standby, 1 = MotorBurn, 2 = Coast, 3 = Freefall, 4 = Landed.
    pub flight_state: Option<u8>,
    /// Received signal strength (dBm) — RX only.
    pub rssi: Option<i16>,
    /// Signal-to-noise ratio (dB) — RX only.
    pub snr: Option<f32>,
    /// Average GPS SNR on the rocket (Sirius) — populated for RX downlink rows.
    pub rocket_gps_snr: Option<u8>,
    /// Average GPS SNR at the base station (Sopdet) — populated for all rows.
    pub base_gps_snr: Option<u8>,

    // ── Uplink / fragment fields (TX only) ────────────────────────────────────
    /// Ground command bundled with this uplink (`"None"`, `"EmergencyLocate"`,
    /// `"DeployEjectionCharge"`, `"EmergencyLocateOff"`).
    pub command: Option<String>,
    /// Number of RTCM correction bytes in the payload.
    pub rtk_data_len: Option<usize>,
    /// Fragment session identifier (FRAGMENT / large-RTK packets only).
    pub fragment_session: Option<u8>,
    /// Zero-based fragment index within the session.
    pub fragment_index: Option<u8>,
    /// Total number of fragments in the session.
    pub fragment_total: Option<u8>,
}

/// One entry in the HTTP access log.
///
/// Written as a single human-readable line:
/// `[timestamp_ms] METHOD /path client:port -> CODE`
#[derive(Debug, Clone)]
pub struct AccessLogEntry {
    /// Milliseconds since the Unix epoch.
    pub timestamp_ms: u64,
    /// HTTP method (`"GET"`, `"POST"`, …).
    pub method: String,
    /// Request path (without query string).
    pub path: String,
    /// Remote address of the client (`"ip:port"`).
    pub client_addr: String,
    /// HTTP response status code sent to the client.
    pub response_code: u16,
}

impl AccessLogEntry {
    /// Format as a single log line (no trailing newline).
    pub fn to_log_line(&self) -> String {
        format!(
            "[{}] {} {} {} -> {}",
            self.timestamp_ms, self.method, self.path, self.client_addr, self.response_code,
        )
    }
}

// ── Logger handle ─────────────────────────────────────────────────────────────

/// Cloneable handle to the two background CSV writer threads.
///
/// Cloning a [`Logger`] is cheap — it only duplicates two `mpsc::Sender`
/// channel endpoints, which are reference-counted internally.
#[derive(Clone)]
pub struct Logger {
    telemetry_tx: Sender<TelemetryLogEntry>,
    access_tx: Sender<AccessLogEntry>,
}

impl Logger {
    /// Create both log files and spawn the background writer threads.
    ///
    /// Returns a [`Logger`] handle that can be cloned freely and passed to
    /// multiple threads.
    pub fn start(telemetry_path: String, access_path: String) -> Result<Self> {
        let (tel_tx, tel_rx) = mpsc::channel::<TelemetryLogEntry>();
        let (acc_tx, acc_rx) = mpsc::channel::<AccessLogEntry>();

        thread::Builder::new()
            .name("telemetry-logger".to_string())
            .spawn(move || run_telemetry_logger(telemetry_path, tel_rx))?;

        thread::Builder::new()
            .name("access-logger".to_string())
            .spawn(move || run_access_logger(access_path, acc_rx))?;

        Ok(Self {
            telemetry_tx: tel_tx,
            access_tx: acc_tx,
        })
    }

    /// Enqueue a telemetry log entry for writing.
    ///
    /// Non-blocking: the entry is sent to the background thread via a channel.
    /// Logs an error if the channel has been closed (background thread exited).
    pub fn log_telemetry(&self, entry: TelemetryLogEntry) {
        if let Err(e) = self.telemetry_tx.send(entry) {
            log::error!("Telemetry log channel closed: {}", e);
        }
    }

    /// Enqueue an HTTP access log entry for writing.
    ///
    /// Non-blocking; same semantics as [`log_telemetry`](Self::log_telemetry).
    pub fn log_access(&self, entry: AccessLogEntry) {
        if let Err(e) = self.access_tx.send(entry) {
            log::error!("Access log channel closed: {}", e);
        }
    }
}

// ── Background writer threads ─────────────────────────────────────────────────

/// Telemetry CSV writer loop.
///
/// Flushes every 10 rows to bound data loss without hammering the filesystem.
fn run_telemetry_logger(path: String, rx: Receiver<TelemetryLogEntry>) {
    let file = match File::create(&path) {
        Ok(f) => f,
        Err(e) => {
            log::error!("Cannot create telemetry log '{}': {}", path, e);
            return;
        }
    };

    let mut writer = csv::Writer::from_writer(BufWriter::new(file));
    let mut row_count: u32 = 0;

    for entry in rx {
        if let Err(e) = writer.serialize(&entry) {
            log::error!("Telemetry CSV write error: {}", e);
        }
        row_count += 1;
        if row_count % 10 == 0 {
            if let Err(e) = writer.flush() {
                log::error!("Telemetry CSV flush error: {}", e);
            }
        }
    }

    // Channel closed — drain and flush before exiting.
    let _ = writer.flush();
    log::info!(
        "Telemetry logger thread exiting ({} rows written)",
        row_count
    );
}

/// HTTP access log writer loop.
///
/// Opens `path` in **append** mode so that all runs write to the same file.
/// Each entry is written as a single human-readable line and flushed
/// immediately (access events are infrequent, so the overhead is negligible).
fn run_access_logger(path: String, rx: Receiver<AccessLogEntry>) {
    let file = match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(f) => f,
        Err(e) => {
            log::error!("Cannot open access log '{}': {}", path, e);
            return;
        }
    };

    let mut writer = BufWriter::new(file);
    let mut line_count: u32 = 0;

    for entry in rx {
        let line = entry.to_log_line();
        if let Err(e) = writeln!(writer, "{}", line) {
            log::error!("Access log write error: {}", e);
        }
        line_count += 1;
        if let Err(e) = writer.flush() {
            log::error!("Access log flush error: {}", e);
        }
    }

    let _ = writer.flush();
    log::info!(
        "Access logger thread exiting ({} lines written)",
        line_count
    );
}
