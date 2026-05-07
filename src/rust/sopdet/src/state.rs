//! Shared application state for the sopdet ground station.
//!
//! [`AppState`] is held behind an `Arc<Mutex<AppState>>` and accessed by the
//! GPS thread, radio thread, and HTTP server thread.

use std::collections::VecDeque;
use std::time::Instant;

use protocol::GroundCommand;
use rtk::GgaData;

/// Maximum number of downlink telemetry entries kept in memory.
pub const MAX_TELEMETRY_HISTORY: usize = 1000;

/// A single received telemetry snapshot from the rocket.
#[derive(Debug, Clone)]
pub struct TelemetryEntry {
    /// Milliseconds since the Unix epoch at which this packet was received.
    pub received_at: u64,
    pub sequence_num: u16,
    /// Milliseconds since rocket boot.
    pub timestamp_ms: u32,
    pub altitude_m: f32,
    pub velocity_mps: f32,
    pub accel_z_gs: f32,
    pub gps_lat: f64,
    pub gps_lon: f64,
    pub gps_alt_m: f32,
    /// Human-readable RTK fix type (e.g. "RTK-Fixed", "GPS", "NoFix").
    pub rtk_fix: String,
    pub pyro_deployed: bool,
    pub pyro_continuity: bool,
    /// 0 = Standby, 1 = MotorBurn, 2 = Coast, 3 = Freefall, 4 = Landed.
    pub flight_state: u8,
    /// Signal strength of the received packet (dBm).
    pub rssi: i16,
    /// Signal-to-noise ratio of the received packet (dB).
    pub snr: f32,
    /// Average GPS SNR on the rocket (dB-Hz), from NMEA GSV sentences via downlink.
    pub gps_snr: u8,
}

/// Shared state accessed by all sopdet threads.
pub struct AppState {
    /// Ring buffer of received downlink packets (newest at the back).
    pub telemetry: Vec<TelemetryEntry>,
    /// Commands queued by the HTTP server, waiting to be transmitted to the rocket.
    pub pending_commands: VecDeque<GroundCommand>,
    /// Latest GPS position fix reported by the base-station LC29H module.
    pub latest_gps: Option<GgaData>,
    /// True once the survey-in has fully converged (`SVIN valid == 2`).
    pub svin_complete: bool,
    /// True while a survey-in is actively in progress (`SVIN valid == 1`).
    pub svin_active: bool,
    /// RSSI of the most recently received downlink packet (dBm).
    pub last_downlink_rssi: Option<i16>,
    /// Instant at which sopdet started — used to compute uptime.
    pub uptime_start: Instant,
    /// Set to `true` by the HTTP server when the operator requests a fresh
    /// survey-in (e.g. after moving the base station).  Cleared by the GPS
    /// thread once the re-survey has been initiated.
    pub resurvey_requested: bool,
    /// Average SNR of the base-station GPS satellites (dB-Hz), from NMEA GSV.
    /// Updated by the GPS thread. Zero until first GSV data arrives.
    pub gps_snr: u8,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            telemetry: Vec::with_capacity(MAX_TELEMETRY_HISTORY),
            pending_commands: VecDeque::new(),
            latest_gps: None,
            svin_complete: false,
            svin_active: false,
            last_downlink_rssi: None,
            uptime_start: Instant::now(),
            resurvey_requested: false,
            gps_snr: 0,
        }
    }

    /// Append a telemetry entry, evicting the oldest entry when the buffer is full.
    pub fn add_telemetry(&mut self, entry: TelemetryEntry) {
        if self.telemetry.len() >= MAX_TELEMETRY_HISTORY {
            self.telemetry.remove(0);
        }
        self.telemetry.push(entry);
    }
}
