//! Flight state machine for Sirius.
//!
//! All transitions are **instantaneous** — no vote debouncing.
//!
//! # Transition conditions
//!
//! | From        | To          | Condition                                                |
//! |-------------|-------------|----------------------------------------------------------|
//! | Standby     | MotorBurn   | altitude > 10 m  AND  raw Z-accel > 20 m/s² (≈ 2.04 g) |
//! | MotorBurn   | Coast       | current velocity < 98 % of peak velocity seen so far    |
//! | Coast       | Freefall    | velocity < 0 m/s  OR  altitude < 99 % of peak altitude  |
//! | Freefall    | Landed      | |Z-accel| spike > 30 m/s² (≈ 3.06 g) — landing impact    |
//!
//! `apogee_m` is the maximum altitude recorded during MotorBurn and Coast.

use firm_core::firm_packets::ProcessedFIRMData;

// ── Threshold constants ───────────────────────────────────────────────────────

/// Minimum altitude (m AGL) required before launch is declared.
const LAUNCH_ALT_THRESHOLD_M: f32 = 30.0;

/// Minimum raw Z-acceleration (g) for launch detection.
const LAUNCH_ACCEL_THRESHOLD_G: f32 = 20.0 / 9.81;

/// Fraction of peak velocity below which burnout / coast is declared.
/// Requires peak velocity > MIN_PEAK_VEL_MPS to prevent a false trigger
/// immediately at launch when max_vel is still near zero.
const COAST_VEL_FRACTION: f32 = 0.98;

/// Minimum peak velocity (m/s) before the MotorBurn → Coast rule activates.
/// Prevents a false coast declaration right at lift-off.
const MIN_PEAK_VEL_MPS: f32 = 10.0;

/// Fraction of peak altitude below which freefall is declared.
const FREEFALL_ALT_FRACTION: f32 = 0.99;

/// |Z-accel| threshold (g) for landing-impact detection.
const LANDING_ACCEL_THRESHOLD_G: f32 = 25.0 / 9.81;

// ── FlightState ───────────────────────────────────────────────────────────────

/// All possible flight states in chronological order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightState {
    /// On the pad, awaiting launch.
    Standby = 0,
    /// Motor is burning; strong upward acceleration detected.
    MotorBurn = 1,
    /// Motor has burned out; rocket is coasting upward.
    Coast = 2,
    /// Past apogee; descending.  Ejection charge fires on entering this state.
    Freefall = 3,
    /// Back on the ground; large deceleration spike detected.
    Landed = 4,
}

impl FlightState {
    /// Return the `u8` discriminant used in telemetry packets.
    #[inline]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub const fn abbrev(self) -> char {
        match self {
            FlightState::Standby => 'S',
            FlightState::MotorBurn => 'M',
            FlightState::Coast => 'C',
            FlightState::Freefall => 'F',
            FlightState::Landed => 'L',
        }
    }
}

impl std::fmt::Display for FlightState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlightState::Standby => write!(f, "Standby"),
            FlightState::MotorBurn => write!(f, "MotorBurn"),
            FlightState::Coast => write!(f, "Coast"),
            FlightState::Freefall => write!(f, "Freefall"),
            FlightState::Landed => write!(f, "Landed"),
        }
    }
}

// ── StateChecker ──────────────────────────────────────────────────────────────

/// Stateful flight state machine.
///
/// Feed every [`ProcessedFIRMData`] packet in order via [`StateChecker::update`].
pub struct StateChecker {
    state: FlightState,

    /// Highest altitude observed.  Updated continuously during MotorBurn and
    /// Coast so it represents true apogee at the moment of Freefall transition.
    max_alt_m: f32,

    /// Highest vertical velocity observed during MotorBurn.  Used to detect
    /// burnout by watching for velocity to fall below 98 % of this value.
    max_velocity_mps: f32,

    /// Time spent in the current state (seconds).
    time_in_state_s: f64,
    state_start_time_s: f64,
}

impl StateChecker {
    /// Create a new checker starting in [`FlightState::Standby`].
    pub fn new() -> Self {
        StateChecker {
            state: FlightState::Standby,
            max_alt_m: 0.0,
            max_velocity_mps: 0.0,
            time_in_state_s: 0.0,
            state_start_time_s: -1.0,
        }
    }

    /// Current flight state.
    #[inline]
    pub fn state(&self) -> FlightState {
        self.state
    }

    /// Peak altitude recorded during ascent (metres AGL).
    #[inline]
    pub fn apogee_m(&self) -> f32 {
        self.max_alt_m
    }

    /// Peak vertical velocity recorded during motor burn (m/s).
    #[inline]
    pub fn max_velocity_mps(&self) -> f32 {
        self.max_velocity_mps
    }

    /// Feed a new data packet and evaluate transition conditions.
    ///
    /// Returns `true` if the state changed as a result of this call.
    pub fn update(&mut self, data: &ProcessedFIRMData) -> bool {
        let alt = data.est_position_z_meters;
        let vel = data.est_velocity_z_meters_per_s;
        // raw_rotated_acceleration_z_gs is in g-units (includes reaction to gravity).
        let accel_z = data.raw_rotated_acceleration_z_gs;

        let prev_state = self.state;

        // Update time in state:
        if self.state_start_time_s < 0.0 {
            // First-ever packet; initialize state start time.
            self.state_start_time_s = data.timestamp_seconds;
        } else {
            self.time_in_state_s = data.timestamp_seconds - self.state_start_time_s;
        }

        match self.state {
            // ── Standby → MotorBurn ──────────────────────────────────────────
            // Trigger: altitude clearly above pad level AND strong upward thrust.
            FlightState::Standby => {
                if alt > LAUNCH_ALT_THRESHOLD_M && accel_z > LAUNCH_ACCEL_THRESHOLD_G {
                    self.state = FlightState::MotorBurn;
                    self.max_alt_m = alt;
                    self.time_in_state_s = 0.0;
                    self.state_start_time_s = data.timestamp_seconds;
                    self.max_velocity_mps = vel.max(0.0);
                    log::info!(
                        "STATE → MotorBurn  alt={:.1} m  accel_z={:.2} g  vel={:.1} m/s",
                        alt,
                        accel_z,
                        vel
                    );
                }
            }

            // ── MotorBurn → Coast ────────────────────────────────────────────
            FlightState::MotorBurn => {
                // Keep rolling maximum.
                if vel > self.max_velocity_mps {
                    self.max_velocity_mps = vel;
                }
                if alt > self.max_alt_m {
                    self.max_alt_m = alt;
                }

                if self.time_in_state_s > 5.0
                // Motor has 6 second burn time
                {
                    self.state = FlightState::Coast;
                    self.time_in_state_s = 0.0;
                    self.state_start_time_s = data.timestamp_seconds;
                    log::info!(
                        "STATE → Coast  vel={:.1} m/s  peak_vel={:.1} m/s  alt={:.1} m",
                        vel,
                        self.max_velocity_mps,
                        alt
                    );
                }
            }

            // ── Coast → Freefall ─────────────────────────────────────────────
            // Trigger: altitude has dropped below 99 % of peak (pressure altitude
            // is reliable) OR velocity has gone negative (past apogee).
            // Velocity is noisier, so it is an OR rather than the sole condition.
            FlightState::Coast => {
                if alt > self.max_alt_m {
                    self.max_alt_m = alt;
                }

                if alt < FREEFALL_ALT_FRACTION * self.max_alt_m || vel < 0.0 {
                    self.state = FlightState::Freefall;
                    self.time_in_state_s = 0.0;
                    self.state_start_time_s = data.timestamp_seconds;
                    log::info!(
                        "STATE → Freefall  vel={:.1} m/s  alt={:.1} m  apogee={:.1} m",
                        vel,
                        alt,
                        self.max_alt_m
                    );
                }
            }

            // ── Freefall → Landed ────────────────────────────────────────────
            // Trigger: sharp deceleration spike characteristic of ground impact
            // (or parachute jerk), regardless of direction.
            FlightState::Freefall => {
                if accel_z.abs() > LANDING_ACCEL_THRESHOLD_G {
                    self.state = FlightState::Landed;
                    self.time_in_state_s = 0.0;
                    self.state_start_time_s = data.timestamp_seconds;
                    log::info!(
                        "STATE → Landed  |accel_z|={:.2} g  alt={:.1} m  apogee={:.1} m",
                        accel_z.abs(),
                        alt,
                        self.max_alt_m
                    );
                }
            }

            // ── Landed: terminal state ───────────────────────────────────────
            FlightState::Landed => {}
        }

        self.state != prev_state
    }
}

impl Default for StateChecker {
    fn default() -> Self {
        Self::new()
    }
}
