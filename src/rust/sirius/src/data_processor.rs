//! Flight parameter storage and FIRM packet processing.
//!
//! [`FlightData`] is a plain struct (no locks, no channels) that holds the
//! latest snapshot of every flight parameter.  It is owned exclusively by the
//! main thread and updated on every iteration of the main loop.
//!
//! [`DataProcessor`] wraps the [`StateChecker`] state machine and processes
//! batches of [`ProcessedFIRMData`] packets returned by the FIRM IMU client,
//! writing results directly into a caller-supplied [`FlightData`].
//!
//! GPS fields (`gps_lat`, `gps_lon`, `gps_alt_m`, `rtk_fix`, `hdop`,
//! `satellites_used`) are **not** updated here — the main loop updates them
//! directly from the LC29H GPS module's NMEA stream.

use firm_core::firm_packets::ProcessedFIRMData;

use protocol::RtkFixType;

use crate::state_machine::{FlightState, StateChecker};

// ── FlightData ────────────────────────────────────────────────────────────────

/// Complete snapshot of all in-flight parameters at a given instant.
///
/// All fields have sensible zero / default values so the struct can be
/// constructed with [`FlightData::default()`] before the first sensor data
/// arrives.
///
/// This struct is intentionally **not** behind a mutex in the main thread.
/// The radio thread receives a cheap [`Clone`] via an
/// `Arc<Mutex<FlightData>>` that is held for only the duration of the clone.
#[derive(Debug, Clone)]
pub struct FlightData {
    // ── FIRM: Kalman-filtered estimates ──────────────────────────────────────
    /// Timestamp from the FIRM packet (seconds since FIRM boot).
    pub timestamp_s: f64,
    /// Estimated altitude above ground level (metres).
    pub altitude_m: f32,
    /// Estimated vertical velocity (m/s, positive = upward).
    pub velocity_mps: f32,

    // ── FIRM: calibrated (rotated) body-frame acceleration (g) ───────────────
    pub accel_x_gs: f32,
    pub accel_y_gs: f32,
    pub accel_z_gs: f32,

    // ── FIRM: raw (unrotated) accelerometer readings (g) ─────────────────────
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
    /// Estimated tilt angle from vertical (degrees).
    pub tilt_deg: f32,
    /// Estimated Mach number.
    pub mach: f32,

    // ── FIRM: attitude quaternion (w, x, y, z) ────────────────────────────────
    pub quat_w: f32,
    pub quat_x: f32,
    pub quat_y: f32,
    pub quat_z: f32,

    // ── GPS / RTK (updated from LC29H NMEA stream) ────────────────────────────
    /// GPS latitude (decimal degrees, positive = North).
    pub gps_lat: f64,
    /// GPS longitude (decimal degrees, positive = East).
    pub gps_lon: f64,
    /// GPS altitude above mean sea level (metres).
    pub gps_alt_m: f32,
    /// Full GPS/RTK fix type.
    pub rtk_fix: RtkFixType,
    /// Horizontal dilution of precision.
    pub hdop: f32,
    /// Number of satellites used in the position solution.
    pub satellites_used: u8,
    /// Average GPS signal-to-noise ratio across tracked satellites (dB-Hz).
    /// Updated from NMEA GSV sentences. Zero until first GSV data arrives.
    pub gps_snr: u8,
    /// Per-constellation GPS SNR — only populated when GSV is available.
    pub gps_snr_gps: u8, // GP
    pub gps_snr_glonass: u8, // GL
    pub gps_snr_galileo: u8, // GA
    pub gps_snr_beidou: u8,  // GB
    pub gps_snr_qzss: u8,    // GQ

    // ── State machine outputs ─────────────────────────────────────────────────
    /// Current flight phase.
    pub flight_state: FlightState,
    /// Maximum altitude recorded during ascent (metres AGL).
    pub apogee_m: f32,

    // ── Pyro ──────────────────────────────────────────────────────────────────
    /// True when GPIO 27 reads high (continuity present on the pyro channel).
    pub pyro_continuity: bool,
    /// True once the ejection charge has been fired.
    pub pyro_deployed: bool,
}

impl Default for FlightData {
    fn default() -> Self {
        FlightData {
            timestamp_s: 0.0,
            altitude_m: 0.0,
            velocity_mps: 0.0,

            accel_x_gs: 0.0,
            accel_y_gs: 0.0,
            accel_z_gs: 0.0,

            raw_accel_x_gs: 0.0,
            raw_accel_y_gs: 0.0,
            raw_accel_z_gs: 0.0,

            gyro_x_dps: 0.0,
            gyro_y_dps: 0.0,
            gyro_z_dps: 0.0,

            mag_x_ut: 0.0,
            mag_y_ut: 0.0,
            mag_z_ut: 0.0,

            temperature_c: 0.0,
            pressure_pa: 0.0,

            tilt_deg: 0.0,
            mach: 0.0,

            // Identity quaternion — rocket is upright, no rotation.
            quat_w: 1.0,
            quat_x: 0.0,
            quat_y: 0.0,
            quat_z: 0.0,

            gps_lat: 0.0,
            gps_lon: 0.0,
            gps_alt_m: 0.0,
            rtk_fix: RtkFixType::NoFix,
            hdop: 99.9,
            satellites_used: 0,
            gps_snr: 0,
            gps_snr_gps: 0,
            gps_snr_glonass: 0,
            gps_snr_galileo: 0,
            gps_snr_beidou: 0,
            gps_snr_qzss: 0,

            flight_state: FlightState::Standby,
            apogee_m: 0.0,

            pyro_continuity: false,
            pyro_deployed: false,
        }
    }
}

// ── DataProcessor ─────────────────────────────────────────────────────────────

/// Processes batches of FIRM IMU packets and updates a [`FlightData`] in place.
///
/// Owns the [`StateChecker`] state machine.  Callers are responsible for
/// everything else (logging, pyro firing, GPS updates, etc.).
pub struct DataProcessor {
    checker: StateChecker,
    altitude_zero_offset_m: f32,
    last_raw_altitude_m: f32,
}

impl DataProcessor {
    /// Create a new processor starting from [`FlightState::Standby`].
    pub fn new() -> Self {
        DataProcessor {
            checker: StateChecker::new(),
            altitude_zero_offset_m: 0.0,
            last_raw_altitude_m: 0.0,
        }
    }

    /// Feed a batch of FIRM packets into the state machine and write all
    /// telemetry fields into `data`.
    ///
    /// Processes every packet in `packets` in order, so `data` will reflect the
    /// **last** packet in the batch after the call returns.
    ///
    /// Returns `true` if the flight state changed on **any** packet in the
    /// batch (i.e. a state transition occurred).
    pub fn update(&mut self, packets: &[ProcessedFIRMData], data: &mut FlightData) -> bool {
        let mut any_state_changed = false;

        for pkt in packets {
            // ── State machine ─────────────────────────────────────────────────
            self.last_raw_altitude_m = pkt.est_position_z_meters;
            let zeroed_altitude_m = pkt.est_position_z_meters - self.altitude_zero_offset_m;

            if self.checker.update_with_altitude(pkt, zeroed_altitude_m) {
                any_state_changed = true;
            }

            // ── FIRM: Kalman estimates ────────────────────────────────────────
            data.timestamp_s = pkt.timestamp_seconds;
            data.altitude_m = zeroed_altitude_m;
            data.velocity_mps = pkt.est_velocity_z_meters_per_s;

            // ── FIRM: calibrated body-frame acceleration ──────────────────────
            data.accel_x_gs = pkt.raw_rotated_acceleration_x_gs;
            data.accel_y_gs = pkt.raw_rotated_acceleration_y_gs;
            data.accel_z_gs = pkt.raw_rotated_acceleration_z_gs;

            // ── FIRM: raw accelerometer ───────────────────────────────────────
            data.raw_accel_x_gs = pkt.raw_acceleration_x_gs;
            data.raw_accel_y_gs = pkt.raw_acceleration_y_gs;
            data.raw_accel_z_gs = pkt.raw_acceleration_z_gs;

            // ── FIRM: gyroscope ───────────────────────────────────────────────
            data.gyro_x_dps = pkt.raw_angular_rate_x_deg_per_s;
            data.gyro_y_dps = pkt.raw_angular_rate_y_deg_per_s;
            data.gyro_z_dps = pkt.raw_angular_rate_z_deg_per_s;

            // ── FIRM: magnetometer ────────────────────────────────────────────
            data.mag_x_ut = pkt.magnetic_field_x_microteslas;
            data.mag_y_ut = pkt.magnetic_field_y_microteslas;
            data.mag_z_ut = pkt.magnetic_field_z_microteslas;

            // ── FIRM: environment ─────────────────────────────────────────────
            data.temperature_c = pkt.temperature_celsius;
            data.pressure_pa = pkt.pressure_pascals;

            // ── FIRM: derived scalars ─────────────────────────────────────────
            data.tilt_deg = pkt.est_tilt_angle_degrees;
            data.mach = pkt.est_mach_number;

            // ── FIRM: attitude quaternion ─────────────────────────────────────
            data.quat_w = pkt.est_quaternion_w;
            data.quat_x = pkt.est_quaternion_x;
            data.quat_y = pkt.est_quaternion_y;
            data.quat_z = pkt.est_quaternion_z;

            // ── State machine outputs ─────────────────────────────────────────
            data.flight_state = self.checker.state();

            let apogee = self.checker.apogee_m();
            if apogee > data.apogee_m {
                data.apogee_m = apogee;
            }
        }

        any_state_changed
    }

    /// Direct access to the underlying state checker (e.g. to read peak
    /// velocity or apogee at any time without calling `update`).
    pub fn checker(&self) -> &StateChecker {
        &self.checker
    }

    /// Make the latest pressure-derived altitude read as zero from now on.
    pub fn zero_altitude_reference(&mut self, data: &mut FlightData) {
        self.altitude_zero_offset_m = self.last_raw_altitude_m;
        data.altitude_m = 0.0;
        data.apogee_m = data.apogee_m.max(0.0);
        log::info!(
            "Altitude zeroed at raw pressure altitude {:.4} m",
            self.altitude_zero_offset_m
        );
    }
}

impl Default for DataProcessor {
    fn default() -> Self {
        Self::new()
    }
}
