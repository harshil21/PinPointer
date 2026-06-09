//! Shared application state for the sopdet ground station.

use std::collections::VecDeque;
use std::time::Instant;

use protocol::GroundCommand;
use rtk::GgaData;

/// Maximum number of downlink telemetry entries kept in memory.
pub const MAX_TELEMETRY_HISTORY: usize = 1000;

/// Per-constellation GPS SNR snapshot.
///
/// Each field holds the most-recent average SNR (dB-Hz) reported by the
/// corresponding NMEA GSV constellation.  A value of 0 means no data has
/// been received yet for that constellation.
#[derive(Debug, Clone, Default)]
pub struct GpsConstellationSnr {
    pub gps: u8,     // GP – GPS / NAVSTAR
    pub glonass: u8, // GL – GLONASS
    pub galileo: u8, // GA – Galileo
    pub beidou: u8,  // GB – BeiDou
    pub qzss: u8,    // GQ – QZSS
    pub navic: u8,   // GI – NavIC / IRNSS
}

impl GpsConstellationSnr {
    /// Average SNR of every active (non-zero) primary constellation.
    ///
    /// QZSS and NavIC are excluded from the average because they are regional
    /// augmentation systems and typically have far fewer tracked satellites,
    /// which would skew the average.
    pub fn average_active(&self) -> u8 {
        let values = [self.gps, self.glonass, self.galileo, self.beidou];
        let (sum, count) = values
            .iter()
            .copied()
            .filter(|&v| v > 0)
            .fold((0u32, 0u32), |(s, c), v| (s + v as u32, c + 1));
        if count == 0 { 0 } else { (sum / count) as u8 }
    }
}

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
    /// Average GPS signal-to-noise ratio on the rocket (dB-Hz), from NMEA GSV.
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
    /// True once the survey-in has fully converged.
    pub svin_complete: bool,
    /// True while a survey-in is actively in progress.
    pub svin_active: bool,
    /// RSSI of the most recently received downlink packet (dBm).
    pub last_downlink_rssi: Option<i16>,
    /// Instant at which sopdet started — used to compute uptime.
    pub uptime_start: Instant,
    /// Set to `true` by the HTTP server when the operator requests a fresh survey-in.
    pub resurvey_requested: bool,
    /// User-configured survey-in minimum duration (seconds). Default 150 s.
    pub svin_min_duration_s: u32,
    /// Live survey-in metrics from \$PQTMSVINSTATUS (updated every second during survey-in).
    pub svin_accuracy_m: f32,
    pub svin_observations: u32,
    pub svin_elapsed_s: u32,
    /// Per-constellation GPS SNR from NMEA GSV sentences at the base station.
    pub gps_snr: GpsConstellationSnr,
    /// Per-constellation GPS SNR from the rocket (via debug packets). None until first packet.
    pub rocket_debug_snr: Option<RocketDebugSnr>,
}

/// Per-constellation SNR received from the rocket via [`protocol::DebugDownlinkPacket`].
#[derive(Debug, Clone, Default)]
pub struct RocketDebugSnr {
    pub gps: u8,
    pub glonass: u8,
    pub galileo: u8,
    pub beidou: u8,
    pub qzss: u8,
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
            svin_min_duration_s: 150,
            svin_accuracy_m: 0.0,
            svin_observations: 0,
            svin_elapsed_s: 0,
            gps_snr: GpsConstellationSnr::default(),
            rocket_debug_snr: None,
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
