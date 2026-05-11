//! # rfm95
//!
//! A Linux SPI driver for the **RFM95W** / **SX1276** LoRa radio module.
//!
//! This library provides a high-level interface for configuring and
//! operating the radio in **LoRa** mode over a Linux SPI bus, using
//! `spidev` for SPI and `gpio-cdev` for reset/interrupt GPIO lines.
//!
//! ## Quick start
//!
//! ```no_run
//! use rfm95::{Rfm95, LoraConfig, PinConfig};
//! use std::time::Duration;
//!
//! let pins = PinConfig {
//!     gpio_chip: "/dev/gpiochip0".into(),
//!     reset_pin: 25,
//!     dio0_pin: Some(24),
//! };
//!
//! let mut radio = Rfm95::open("/dev/spidev0.0", pins).unwrap();
//! radio.configure(&LoraConfig::default()).unwrap();
//! radio.transmit(b"Hello LoRa!").unwrap();
//!
//! match radio.receive(Duration::from_secs(5)) {
//!     Ok(packet) => {
//!         println!("Received {} bytes, RSSI={} dBm", packet.payload.len(), packet.rssi);
//!     }
//!     Err(e) => eprintln!("RX error: {e}"),
//! }
//! ```
//!
//! ## Features
//!
//! - Full LoRa modem configuration (BW, SF, CR, preamble, sync word, IQ inversion, …)
//! - Single-shot and continuous receive modes
//! - Channel Activity Detection (CAD)
//! - Configurable power amplifier (PA_BOOST / RFO, +20 dBm high-power mode)
//! - RSSI and SNR readback
//! - IRQ flag management

pub mod config;
pub mod error;
pub mod radio;
pub mod registers;

// Re-export the most commonly used types at crate root.
pub use config::*;
pub use error::{Result, Rfm95Error};
pub use radio::{PinConfig, ReceivedPacket, Rfm95};
pub use registers::IrqFlags;
