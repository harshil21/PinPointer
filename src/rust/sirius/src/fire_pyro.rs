use anyhow::{Context, Result};
use gpio_cdev::{Chip, LineRequestFlags};
use std::thread;
use std::time::Duration;

/// Linux GPIO character device used by `gpio-cdev`.
const GPIO_CHIP: &str = "/dev/gpiochip0";

/// Raspberry Pi GPIO 22 (Pin 15 on the 40-pin header) — Pyro ejection charge.
const GPIO_PYRO: u32 = 22;

/// Ejection charge activation time.
/// Mirroring main.rs logic, 8ms pulse.
const PYRO_PULSE_MS: u64 = 1000;

fn main() -> Result<()> {
    env_logger::init();

    println!(
        "WARNING: This will immediately fire the pyro channel (GPIO {}) for {} ms.",
        GPIO_PYRO, PYRO_PULSE_MS
    );
    println!("Please make sure you are clear of any explosive charges.");
    println!("Firing in 3 seconds...");
    thread::sleep(Duration::from_secs(3));

    let mut chip =
        Chip::new(GPIO_CHIP).with_context(|| format!("Cannot open GPIO chip '{}'", GPIO_CHIP))?;

    let pyro_pin = chip
        .get_line(GPIO_PYRO)
        .with_context(|| format!("GPIO {} (pyro) unavailable", GPIO_PYRO))?
        .request(LineRequestFlags::OUTPUT, 0, "fire-pyro-test")
        .with_context(|| format!("Cannot claim GPIO {} as output", GPIO_PYRO))?;

    println!("*** FIRING PYRO CHANNEL ***");

    pyro_pin
        .set_value(1)
        .expect("CRITICAL: failed to set pyro pin high");

    thread::sleep(Duration::from_millis(PYRO_PULSE_MS));

    pyro_pin
        .set_value(0)
        .expect("CRITICAL: failed to set pyro pin low");

    println!("*** PYRO FIRED ***");

    Ok(())
}
