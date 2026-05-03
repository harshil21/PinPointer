//! Hardware-PWM buzzer driver for Sirius.
//!
//! GPIO 18 on the Raspberry Pi is hardware PWM channel 0 (PWM0).
//! This module drives it via `rppal::pwm` for an accurate 4 kHz tone
//! without burning a CPU core on bit-banging.
//!
//! # Patterns
//!
//! | Pattern                | Behaviour                                                      |
//! |------------------------|----------------------------------------------------------------|
//! | `Silent`               | PWM disabled.                                                  |
//! | `StandbyNoContinuity`  | Two 100 ms beeps, 1.8 s silence, repeat.                       |
//! | `StandbyContinuity`    | 500 ms beep, 500 ms silence, repeat.                           |
//! | `ApogeeAnnounce(alt)`  | Beep each decimal digit of `alt` metres, then 3 s pause.       |
//! | `Emergency`            | Continuous 4 kHz tone; re-checks pattern once per second.      |
//!
//! # Digit encoding for `ApogeeAnnounce`
//!
//! - Digits 1–9 → that many 100 ms pulses separated by 100 ms gaps.
//! - Digit 0   → 10 pulses (avoids ambiguity with silence).
//! - Inter-digit gap: 800 ms.
//! - End-of-number pause: 3 s before repeating.
//!
//! # Raspberry Pi setup
//!
//! Enable hardware PWM in `/boot/config.txt`:
//! ```
//! dtoverlay=pwm,pin=18,func=2
//! ```

use rppal::pwm::{Channel, Polarity, Pwm};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Context;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Buzzer resonant frequency (Hz). The piezo is loudest here.
const BUZZER_FREQ_HZ: f64 = 4_000.0;

/// Duty cycle for maximum volume (50 % = symmetric square wave).
const DUTY_CYCLE: f64 = 0.5;

// ── BuzzerPattern ─────────────────────────────────────────────────────────────

/// Active buzzer behaviour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuzzerPattern {
    /// Completely silent — PWM disabled.
    Silent,
    /// Standby, pyro channel has **no** continuity.
    StandbyNoContinuity,
    /// Standby, pyro channel **has** continuity.
    StandbyContinuity,
    /// Landed: beep out the apogee altitude in decimal digits.
    ApogeeAnnounce(u32),
    /// Emergency locator: continuous 4 kHz tone.
    Emergency,
}

// ── BuzzerController ──────────────────────────────────────────────────────────

/// Thread-safe handle for changing the active buzzer pattern.
pub struct BuzzerController {
    pattern: Arc<Mutex<BuzzerPattern>>,
}

impl BuzzerController {
    /// Open hardware PWM channel 0 (GPIO 18) and spawn the buzzer thread.
    pub fn new() -> anyhow::Result<Self> {
        let pwm = Pwm::with_frequency(
            Channel::Pwm0,
            BUZZER_FREQ_HZ,
            DUTY_CYCLE,
            Polarity::Normal,
            false, // start disabled
        )
        .context(
            "Failed to open PWM channel 0 (GPIO 18). \
             Is 'dtoverlay=pwm,pin=18,func=2' in /boot/config.txt?",
        )?;

        let pattern = Arc::new(Mutex::new(BuzzerPattern::Silent));
        let pattern_clone = Arc::clone(&pattern);

        thread::Builder::new()
            .name("buzzer".to_string())
            .spawn(move || buzzer_loop(pwm, pattern_clone))
            .context("Failed to spawn buzzer thread")?;

        Ok(BuzzerController { pattern })
    }

    /// Replace the active pattern. Takes effect at the next pattern-cycle
    /// boundary (at most one full beep/silence step away).
    pub fn set_pattern(&self, p: BuzzerPattern) {
        *self.pattern.lock().unwrap() = p;
    }

    /// Return a clone of the currently active pattern.
    pub fn get_pattern(&self) -> BuzzerPattern {
        self.pattern.lock().unwrap().clone()
    }
}

// ── Background thread ─────────────────────────────────────────────────────────

fn buzzer_loop(pwm: Pwm, pattern: Arc<Mutex<BuzzerPattern>>) {
    // Make sure the PWM starts silent.
    let _ = pwm.disable();

    loop {
        // Read the current pattern at the top of every cycle.
        // Any change made by the main thread will be visible on the next
        // iteration — no mid-sequence interruption needed.
        let current = pattern.lock().unwrap().clone();

        match current {
            BuzzerPattern::Silent => {
                let _ = pwm.disable();
                thread::sleep(Duration::from_millis(100));
            }

            BuzzerPattern::Emergency => {
                // One second of continuous tone, then loop back to re-check.
                beep(&pwm, 1_000);
            }

            BuzzerPattern::StandbyNoContinuity => {
                beep(&pwm, 100);
                silence(100);
                beep(&pwm, 100);
                silence(1_800);
            }

            BuzzerPattern::StandbyContinuity => {
                beep(&pwm, 500);
                silence(500);
            }

            BuzzerPattern::ApogeeAnnounce(alt_m) => {
                let digits = altitude_digits(alt_m);
                let last = digits.len().saturating_sub(1);
                for (i, &d) in digits.iter().enumerate() {
                    beep_digit(&pwm, d);
                    if i < last {
                        silence(800);
                    }
                }
                // Long pause before repeating the sequence.
                silence(3_000);
            }
        }
    }
}

// ── Tone helpers ──────────────────────────────────────────────────────────────

/// Enable the PWM for `ms` milliseconds, then disable it.
fn beep(pwm: &Pwm, ms: u64) {
    let _ = pwm.enable();
    thread::sleep(Duration::from_millis(ms));
    let _ = pwm.disable();
}

/// Silent gap between pulses or after the final pulse of a digit.
fn silence(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}

/// Emit `digit` short pulses (10 for digit 0 to distinguish from silence).
/// Each pulse is 100 ms on; intra-digit gaps are 100 ms.
fn beep_digit(pwm: &Pwm, digit: u8) {
    let count = if digit == 0 { 10 } else { digit as usize };
    for i in 0..count {
        beep(pwm, 100);
        if i + 1 < count {
            silence(100);
        }
    }
}

// ── Utility ───────────────────────────────────────────────────────────────────

/// Decompose `alt_m` into its decimal digits, most-significant first.
///
/// `0` → `[0]`, `1204` → `[1, 2, 0, 4]`.
fn altitude_digits(alt_m: u32) -> Vec<u8> {
    if alt_m == 0 {
        return vec![0];
    }
    alt_m
        .to_string()
        .chars()
        .map(|c| c.to_digit(10).unwrap_or(0) as u8)
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::altitude_digits;

    #[test]
    fn digits_zero() {
        assert_eq!(altitude_digits(0), vec![0]);
    }

    #[test]
    fn digits_single_nonzero() {
        assert_eq!(altitude_digits(7), vec![7]);
    }

    #[test]
    fn digits_multi() {
        assert_eq!(altitude_digits(1234), vec![1, 2, 3, 4]);
    }

    #[test]
    fn digits_internal_zero() {
        assert_eq!(altitude_digits(1004), vec![1, 0, 0, 4]);
    }

    #[test]
    fn digits_round_thousands() {
        assert_eq!(altitude_digits(3000), vec![3, 0, 0, 0]);
    }
}
