//! Listen for LoRa packets in continuous receive mode.
//!
//! Usage:
//!     cargo run -p rfm95 --example receive
//!
//! Set RUST_LOG=debug for verbose SPI/register output.

use std::thread;
use std::time::Duration;

use rfm95::{
    Bandwidth, CodingRate, LoraConfig, PaConfig, PaSelect, PinConfig, Rfm95, Rfm95Error,
    SpreadingFactor,
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

    // The receiver config MUST match the transmitter.
    let config = LoraConfig {
        frequency: 915_000_000,
        bandwidth: Bandwidth::Bw500kHz,
        spreading_factor: SpreadingFactor::Sf7,
        header_mode: rfm95::HeaderMode::Explicit,
        coding_rate: CodingRate::Cr4_5,
        pa_config: PaConfig {
            pa_select: PaSelect::PaBoost,
            output_power: 17,
        },
        ..LoraConfig::default()
    };

    println!("Configuring radio: {config:#?}");
    radio.configure(&config)?;

    println!("Chip version:      0x{:02x}", radio.version()?);
    println!("Carrier frequency: {} Hz", radio.get_frequency()?);
    println!();

    println!("Listening — press Ctrl-C to stop.\n");
    radio.start_receive_continuous()?;

    let mut packet_count: u64 = 0;
    loop {
        match radio.poll_receive() {
            Ok(Some(packet)) => {
                packet_count += 1;
                let text = String::from_utf8_lossy(&packet.payload);
                println!("┌─ Packet #{packet_count} ─────────────────────────");
                println!("│ Payload ({} bytes): {text:?}", packet.payload.len());
                println!("│ RSSI:  {} dBm", packet.rssi);
                println!("│ SNR:   {:.1} dB", packet.snr);
                println!("└──────────────────────────────────────\n");
            }
            Ok(None) => {
                // Nothing yet — sleep briefly to avoid busy-spinning.
                thread::sleep(Duration::from_millis(10));
            }
            Err(Rfm95Error::CrcError) => {
                eprintln!("⚠ CRC error — packet dropped");
            }
            Err(e) => {
                eprintln!("Error: {e}");
                // Re-enter continuous RX after a transient error.
                radio.start_receive_continuous()?;
            }
        }
    }
}
