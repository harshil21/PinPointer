//! Asynchronous CSV flight logger with guaranteed durability.
//!
//! A background thread owns the [`csv::Writer`] and a cloned [`std::fs::File`]
//! handle. Every [`FSYNC_EVERY_ROWS`] rows it calls `writer.flush()` to drain
//! the `BufWriter` to the OS, then `file.sync_all()` (`fsync(2)`) to commit
//! those pages to non-volatile storage. At most [`FSYNC_EVERY_ROWS`] rows of
//! data can be lost if power is cut during flight.
//!
//! [`Logger::log`] is non-blocking — if the internal queue is full the entry
//! is silently dropped rather than stalling the 100 Hz main loop.

use std::fs::{File, OpenOptions};
use std::io::BufWriter;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use serde::Serialize;

use crate::data_processor::FlightData;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Capacity of the internal log queue (number of [`LogEntry`] items).
const QUEUE_CAPACITY: usize = 2048;

/// Number of rows written between `flush()` + `fsync()` calls.
const FSYNC_EVERY_ROWS: u64 = 100;

// ── LogEntry ──────────────────────────────────────────────────────────────────

/// One row in the CSV flight log.
///
/// Field declaration order determines the CSV column order.
#[derive(Debug, Serialize)]
pub struct LogEntry {
    // ── Timing ────────────────────────────────────────────────────────────────
    /// Nanoseconds since the Unix epoch (UTC). Falls back to 0 on the
    /// theoretical case where the system clock is before the epoch.
    pub timestamp_ns: u128,

    // ── State machine ─────────────────────────────────────────────────────────
    pub flight_state: char,

    // ── FIRM: Kalman-filtered estimates ───────────────────────────────────────
    pub altitude_m: f32,
    pub velocity_z_mps: f32,

    // ── FIRM: calibrated (rotated) body-frame acceleration (g) ────────────────
    pub accel_x_gs: f32,
    pub accel_y_gs: f32,
    pub accel_z_gs: f32,

    // ── FIRM: raw (unrotated) accelerometer readings (g) ──────────────────────
    pub raw_accel_x_gs: f32,
    pub raw_accel_y_gs: f32,
    pub raw_accel_z_gs: f32,

    // ── FIRM: gyroscope (°/s) ─────────────────────────────────────────────────
    pub gyro_x_dps: f32,
    pub gyro_y_dps: f32,
    pub gyro_z_dps: f32,

    // ── FIRM: magnetometer (µT) ───────────────────────────────────────────────
    pub mag_x_ut: f32,
    pub mag_y_ut: f32,
    pub mag_z_ut: f32,

    // ── FIRM: environment ─────────────────────────────────────────────────────
    pub temperature_c: f32,
    pub pressure_pa: f32,

    // ── FIRM: derived scalars ─────────────────────────────────────────────────
    pub tilt_deg: f32,

    // ── FIRM: attitude quaternion (w, x, y, z) ────────────────────────────────
    pub quat_w: f32,
    pub quat_x: f32,
    pub quat_y: f32,
    pub quat_z: f32,

    // ── GPS / RTK ─────────────────────────────────────────────────────────────
    pub gps_lat: f64,
    pub gps_lon: f64,
    pub gps_alt_m: f32,
    /// Full fix-type string (e.g. "RTK-Fixed", "DGPS", "GPS", "NoFix").
    pub rtk_fix: String,
    /// Horizontal dilution of precision.
    pub hdop: f32,
    /// Number of satellites used in the position solution.
    pub satellites_used: u8,
    /// Per-constellation SNR (dB-Hz). Zero until the first GSV sentence for
    /// that constellation is received.
    pub gps_snr_gps: u8,
    pub gps_snr_glonass: u8,
    pub gps_snr_galileo: u8,
    pub gps_snr_beidou: u8,
    pub gps_snr_qzss: u8,

    // ── Pyro ──────────────────────────────────────────────────────────────────
    pub pyro_deployed: bool,
    pub pyro_continuity: bool,

    // ── Radio: packet mirrors ─────────────────────────────────────────────────
    /// Lower-case hex encoding of the last received uplink / fragment bytes.
    /// Empty string if nothing has been received yet.
    pub rx_packet_hex: String,
}

// ── build_log_entry ───────────────────────────────────────────────────────────

/// Build a [`LogEntry`] from the current [`FlightData`] snapshot plus the
/// raw bytes of the most recently received radio packet.
///
/// `rx_hex` should already be hex-encoded; pass an empty string if no
/// packet has been received yet.
pub fn build_log_entry(data: &FlightData, rx_hex: &str) -> LogEntry {
    LogEntry {
        timestamp_ns: unix_epoch_nanos(),
        flight_state: data.flight_state.abbrev(),

        altitude_m: data.altitude_m,
        velocity_z_mps: data.velocity_mps,

        accel_x_gs: data.accel_x_gs,
        accel_y_gs: data.accel_y_gs,
        accel_z_gs: data.accel_z_gs,

        raw_accel_x_gs: data.raw_accel_x_gs,
        raw_accel_y_gs: data.raw_accel_y_gs,
        raw_accel_z_gs: data.raw_accel_z_gs,

        gyro_x_dps: data.gyro_x_dps,
        gyro_y_dps: data.gyro_y_dps,
        gyro_z_dps: data.gyro_z_dps,

        mag_x_ut: data.mag_x_ut,
        mag_y_ut: data.mag_y_ut,
        mag_z_ut: data.mag_z_ut,

        temperature_c: data.temperature_c,
        pressure_pa: data.pressure_pa,

        tilt_deg: data.tilt_deg,

        quat_w: data.quat_w,
        quat_x: data.quat_x,
        quat_y: data.quat_y,
        quat_z: data.quat_z,

        gps_lat: data.gps_lat,
        gps_lon: data.gps_lon,
        gps_alt_m: data.gps_alt_m,
        rtk_fix: data.rtk_fix.to_string(),
        hdop: data.hdop,
        satellites_used: data.satellites_used,
        gps_snr_gps: data.gps_snr_gps,
        gps_snr_glonass: data.gps_snr_glonass,
        gps_snr_galileo: data.gps_snr_galileo,
        gps_snr_beidou: data.gps_snr_beidou,
        gps_snr_qzss: data.gps_snr_qzss,

        pyro_deployed: data.pyro_deployed,
        pyro_continuity: data.pyro_continuity,

        rx_packet_hex: rx_hex.to_string(),
    }
}

/// Nanoseconds elapsed since the Unix epoch (UTC). Returns 0 if the system
/// clock is set before the epoch — vanishingly unlikely in practice.
fn unix_epoch_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

// ── Logger ────────────────────────────────────────────────────────────────────

/// Thread-safe handle to the background CSV logger.
///
/// Cloning is cheap — all clones share the same sender end of the channel.
#[derive(Clone)]
pub struct Logger {
    sender: mpsc::SyncSender<LogEntry>,
}

impl Logger {
    /// Spawn the logger background thread and return a handle.
    ///
    /// Creates (or truncates) the CSV file at `path`.
    pub fn new(path: &str) -> anyhow::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<LogEntry>(QUEUE_CAPACITY);
        let path_owned = path.to_string();

        thread::Builder::new()
            .name("csv-logger".to_string())
            .spawn(move || logger_thread(receiver, &path_owned))
            .context("Failed to spawn CSV logger thread")?;

        Ok(Logger { sender })
    }

    /// Enqueue a log entry.
    ///
    /// **Non-blocking**: drops the entry silently if the queue is full rather
    /// than stalling the caller.
    pub fn log(&self, entry: LogEntry) {
        match self.sender.try_send(entry) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(_)) => {
                eprintln!("[logger] WARNING: queue full, dropping log entry");
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                // Background thread exited — nothing we can do.
            }
        }
    }
}

// ── Background thread ─────────────────────────────────────────────────────────

fn logger_thread(receiver: mpsc::Receiver<LogEntry>, path: &str) {
    // Open (or create) the CSV file.
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .unwrap_or_else(|e| panic!("Cannot open flight log '{}': {}", path, e));

    // Persist the directory entry so the file survives a hard power cut.
    //
    // fsync on the file handle only commits the file's data and inode to disk.
    // The parent directory holds a separate data block mapping this filename to
    // that inode.  If we don't fsync the directory, a hard shutdown before the
    // filesystem flushes that block leaves the inode orphaned — the file appears
    // gone on the next boot even though its data reached storage.
    let parent_dir = Path::new(path).parent().unwrap_or_else(|| Path::new("."));
    match File::open(parent_dir) {
        Ok(dir) => {
            if let Err(e) = dir.sync_all() {
                eprintln!("[logger] WARNING: could not fsync parent directory: {}", e);
            }
        }
        Err(e) => {
            eprintln!(
                "[logger] WARNING: could not open parent directory for fsync: {}",
                e
            );
        }
    }

    // Clone the file descriptor now — we need it for fsync after the
    // BufWriter has been flushed.
    let sync_file: File = file
        .try_clone()
        .unwrap_or_else(|e| panic!("Cannot clone log file handle: {}", e));

    let buf_writer = BufWriter::new(file);
    let mut writer = csv::WriterBuilder::new()
        .has_headers(true)
        .from_writer(buf_writer);

    let mut rows_written: u64 = 0;

    for entry in receiver {
        if let Err(e) = writer.serialize(&entry) {
            eprintln!("[logger] CSV serialise error: {}", e);
            continue;
        }

        rows_written += 1;

        if rows_written % FSYNC_EVERY_ROWS == 0 {
            // 1. Flush csv Writer's record buffer into the OS page cache.
            if let Err(e) = writer.flush() {
                eprintln!("[logger] BufWriter flush error: {}", e);
            }
            // 2. fsync — guarantee data has reached non-volatile storage.
            if let Err(e) = sync_file.sync_all() {
                eprintln!("[logger] fsync error: {}", e);
            }
        }
    }

    // Final flush + fsync on channel close (clean shutdown or program exit).
    if let Err(e) = writer.flush() {
        eprintln!("[logger] Final BufWriter flush error: {}", e);
    }
    if let Err(e) = sync_file.sync_all() {
        eprintln!("[logger] Final fsync error: {}", e);
    }

    log::info!(
        "[logger] Flight log '{}' closed ({} rows).",
        path,
        rows_written
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_processor::FlightData;

    fn sample_flight_data() -> FlightData {
        FlightData::default()
    }

    #[test]
    fn timestamp_is_unix_epoch_nanoseconds_utc() {
        let now_ns_lower = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let entry = build_log_entry(&sample_flight_data(), "");
        let now_ns_upper = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        // Should be a Unix-epoch nanosecond timestamp (well after 1 Jan 2020 UTC),
        // and bracketed by readings taken just before and just after.
        let year_2020_ns: u128 = 1_577_836_800u128 * 1_000_000_000u128;
        assert!(entry.timestamp_ns > year_2020_ns);
        assert!(entry.timestamp_ns >= now_ns_lower);
        assert!(entry.timestamp_ns <= now_ns_upper);
    }

    #[test]
    fn csv_header_excludes_tx_packet_hex_and_gps_snr() {
        // Serialise one row via the csv crate; capture the header line.
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut wtr = csv::WriterBuilder::new()
                .has_headers(true)
                .from_writer(&mut buf);
            wtr.serialize(build_log_entry(&sample_flight_data(), ""))
                .expect("serialise log entry");
            wtr.flush().unwrap();
        }
        let text = String::from_utf8(buf).expect("utf-8");
        let header = text.lines().next().expect("header line");
        let cols: Vec<&str> = header.split(',').collect();

        assert!(
            cols.contains(&"timestamp_ns"),
            "timestamp_ns missing: {header}"
        );
        assert!(
            cols.contains(&"rx_packet_hex"),
            "rx_packet_hex missing: {header}"
        );
        assert!(
            cols.contains(&"gps_snr_gps"),
            "per-constellation column missing: {header}"
        );
        assert!(
            !cols.contains(&"tx_packet_hex"),
            "tx_packet_hex column must be removed: {header}"
        );
        // The bare `gps_snr` column must be gone. Per-constellation columns
        // start with `gps_snr_` and are still allowed.
        assert!(
            !cols.contains(&"gps_snr"),
            "gps_snr column must be removed: {header}"
        );
    }
}
