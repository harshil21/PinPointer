//! Periodically transmit LoRa packets.
//!
//! Usage:
//!     cargo run -p rfm95 --example transmit
//!
//! Set RUST_LOG=debug for verbose SPI/register output.

use std::thread;
use std::time::Duration;

use rfm95::{
    Bandwidth, CodingRate, LoraConfig, PaConfig, PaSelect, PinConfig, Rfm95, SpreadingFactor,
};

/// Adjust these to match your wiring.
const SPI_DEVICE: &str = "/dev/spidev0.0";
const GPIO_CHIP: &str = "/dev/gpiochip0";
const RESET_PIN: u32 = 17;
const DIO0_PIN: u32 = 24;

fn main() -> rfm95::Result<()> {
    env_logger::init();

    let pins = PinConfig {
        gpio_chip: GPIO_CHIP.into(),
        reset_pin: RESET_PIN,
        dio0_pin: Some(DIO0_PIN),
    };

    println!("Opening RFM95 on {SPI_DEVICE} …");
    let mut radio = Rfm95::open(SPI_DEVICE, pins)?;

    let config = LoraConfig {
        frequency: 915_000_000,
        bandwidth: Bandwidth::Bw500kHz,
        spreading_factor: SpreadingFactor::Sf7,
        coding_rate: CodingRate::Cr4_5,
        header_mode: rfm95::HeaderMode::Explicit,
        pa_config: PaConfig {
            pa_select: PaSelect::PaBoost,
            output_power: 20,
        },
        ..LoraConfig::default()
    };

    println!("Configuring radio: {config:#?}");
    radio.configure(&config)?;

    println!("Chip version:      0x{:02x}", radio.version()?);
    println!("Carrier frequency: {} Hz", radio.get_frequency()?);
    println!();

    let mut counter: u32 = 0;
    loop {
        let message = format!("hello #{counter}");
        print!("TX [{:>3} bytes]: {message:?} … ", message.len());

        radio.transmit(message.as_bytes())?;

        println!("✓ sent");
        counter += 1;
        thread::sleep(Duration::from_secs(2));
    }
}