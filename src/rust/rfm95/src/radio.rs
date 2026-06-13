//! Main RFM95/SX1276 radio driver.

use std::thread;
use std::time::{Duration, Instant};

use gpio_cdev::{Chip, LineHandle, LineRequestFlags};
use spidev::{SpiModeFlags, Spidev, SpidevOptions, SpidevTransfer};

use crate::config::*;
use crate::error::{Result, Rfm95Error};
use crate::registers::*;

/// A received LoRa packet with metadata.
#[derive(Debug, Clone)]
pub struct ReceivedPacket {
    /// Raw payload bytes.
    pub payload: Vec<u8>,
    /// RSSI in dBm.
    pub rssi: i16,
    /// SNR in dB.
    pub snr: f32,
}

/// GPIO pin assignments for the RFM95 module.
#[derive(Debug, Clone)]
pub struct PinConfig {
    /// Path to the GPIO chip (e.g. "/dev/gpiochip0").
    pub gpio_chip: String,
    /// GPIO line for the reset pin.
    pub reset_pin: u32,
    /// Optional GPIO line for DIO0 (TX_DONE / RX_DONE).
    pub dio0_pin: Option<u32>,
}

/// Driver for the RFM95W / SX1276 LoRa radio module.
pub struct Rfm95 {
    spi: Spidev,
    reset_line: LineHandle,
    dio0_line: Option<LineHandle>,
    config: LoraConfig,
}

impl Rfm95 {
    /// Open a connection to the RFM95 module, reset it, verify the chip
    /// version, and place the radio in LoRa standby mode.
    pub fn open(spi_path: &str, pin_config: PinConfig) -> Result<Self> {
        let mut spi = Spidev::open(spi_path)?;
        let opts = SpidevOptions::new()
            .bits_per_word(8)
            .max_speed_hz(5_000_000)
            .mode(SpiModeFlags::SPI_MODE_0)
            .build();
        spi.configure(&opts)?;

        let mut chip = Chip::new(&pin_config.gpio_chip)?;
        let reset_line = chip.get_line(pin_config.reset_pin)?.request(
            LineRequestFlags::OUTPUT,
            1,
            "rfm95-reset",
        )?;
        let dio0_line = match pin_config.dio0_pin {
            Some(pin) => Some(chip.get_line(pin)?.request(
                LineRequestFlags::INPUT,
                0,
                "rfm95-dio0",
            )?),
            None => None,
        };

        let mut radio = Self {
            spi,
            reset_line,
            dio0_line,
            config: LoraConfig::default(),
        };

        radio.reset()?;
        radio.verify_version()?;

        // Sleep → enable LoRa bit → Standby
        radio.set_mode(OperatingMode::Sleep)?;
        let opmode = radio.read_register(Register::OpMode)?;
        radio.write_register(Register::OpMode, opmode | OPMODE_LONG_RANGE_MODE)?;
        radio.set_mode(OperatingMode::Standby)?;

        Ok(radio)
    }

    // ── Hardware control ─────────────────────────────────────────

    /// Hardware-reset the module.
    pub fn reset(&mut self) -> Result<()> {
        self.reset_line.set_value(0)?;
        thread::sleep(Duration::from_millis(10));
        self.reset_line.set_value(1)?;
        thread::sleep(Duration::from_millis(10));
        Ok(())
    }

    fn verify_version(&mut self) -> Result<()> {
        let version = self.read_register(Register::Version)?;
        if version != EXPECTED_VERSION {
            return Err(Rfm95Error::UnexpectedVersion {
                expected: EXPECTED_VERSION,
                actual: version,
            });
        }
        log::debug!("RFM95 version verified: 0x{version:02x}");
        Ok(())
    }

    /// Return the chip version byte.
    pub fn version(&mut self) -> Result<u8> {
        self.read_register(Register::Version)
    }

    // ── SPI primitives ───────────────────────────────────────────

    /// Read a single register.
    pub fn read_register(&mut self, reg: Register) -> Result<u8> {
        let tx = [reg.addr() & 0x7F, 0x00];
        let mut rx = [0u8; 2];
        let mut xfer = SpidevTransfer::read_write(&tx, &mut rx);
        self.spi.transfer(&mut xfer)?;
        Ok(rx[1])
    }

    /// Write a single register.
    pub fn write_register(&mut self, reg: Register, value: u8) -> Result<()> {
        let tx = [reg.addr() | SPI_WRITE_MASK, value];
        let mut rx = [0u8; 2];
        let mut xfer = SpidevTransfer::read_write(&tx, &mut rx);
        self.spi.transfer(&mut xfer)?;
        Ok(())
    }

    fn write_burst(&mut self, reg: Register, data: &[u8]) -> Result<()> {
        let mut tx = vec![reg.addr() | SPI_WRITE_MASK];
        tx.extend_from_slice(data);
        let mut rx = vec![0u8; tx.len()];
        let mut xfer = SpidevTransfer::read_write(&tx, &mut rx);
        self.spi.transfer(&mut xfer)?;
        Ok(())
    }

    fn read_burst(&mut self, reg: Register, len: usize) -> Result<Vec<u8>> {
        let mut tx = vec![0u8; len + 1];
        tx[0] = reg.addr() & 0x7F;
        let mut rx = vec![0u8; len + 1];
        let mut xfer = SpidevTransfer::read_write(&tx, &mut rx);
        self.spi.transfer(&mut xfer)?;
        Ok(rx[1..].to_vec())
    }

    // ── Mode control ─────────────────────────────────────────────

    /// Set the radio operating mode.
    pub fn set_mode(&mut self, mode: OperatingMode) -> Result<()> {
        let current = self.read_register(Register::OpMode)?;
        let new = (current & !OPMODE_MODE_MASK) | (mode as u8);
        self.write_register(Register::OpMode, new)?;
        thread::sleep(Duration::from_millis(1));
        Ok(())
    }

    /// Read the current operating mode.
    pub fn get_mode(&mut self) -> Result<OperatingMode> {
        let val = self.read_register(Register::OpMode)? & OPMODE_MODE_MASK;
        match val {
            0x00 => Ok(OperatingMode::Sleep),
            0x01 => Ok(OperatingMode::Standby),
            0x02 => Ok(OperatingMode::FrequencySynthesisTx),
            0x03 => Ok(OperatingMode::Tx),
            0x04 => Ok(OperatingMode::FrequencySynthesisRx),
            0x05 => Ok(OperatingMode::RxContinuous),
            0x06 => Ok(OperatingMode::RxSingle),
            0x07 => Ok(OperatingMode::Cad),
            _ => Err(Rfm95Error::InvalidState),
        }
    }

    /// Place the radio in standby mode.
    pub fn standby(&mut self) -> Result<()> {
        self.set_mode(OperatingMode::Standby)
    }

    /// Place the radio in sleep mode.
    pub fn sleep(&mut self) -> Result<()> {
        self.set_mode(OperatingMode::Sleep)
    }

    // ── Configuration ────────────────────────────────────────────

    /// Apply a full LoRa configuration. The radio is placed in standby.
    pub fn configure(&mut self, config: &LoraConfig) -> Result<()> {
        self.set_mode(OperatingMode::Standby)?;
        self.config = *config;

        // SX1276 §4.1.1: LowFrequencyModeOn (RegOpMode bit 3) must be 0 for
        // the HF port (frequency > ~600 MHz) and 1 for the LF port.
        // open() ORs OPMODE_LONG_RANGE_MODE onto the reset value 0x09, which
        // has bit 3 = 1 (LF mode on by default).  set_mode() only touches
        // bits [2:0], so this bit stays stuck at 1 for the entire session
        // unless explicitly corrected here.  Operating at 915 MHz with
        // LowFrequencyModeOn = 1 uses the wrong internal calibration.
        {
            let opmode = self.read_register(Register::OpMode)?;
            let corrected = if config.frequency > 525_000_000 {
                opmode & !OPMODE_LOW_FREQUENCY_MODE // HF: clear bit 3
            } else {
                opmode | OPMODE_LOW_FREQUENCY_MODE // LF: set bit 3
            };
            if corrected != opmode {
                self.write_register(Register::OpMode, corrected)?;
            }
        }

        self.set_frequency(config.frequency)?;
        self.set_pa_config(&config.pa_config)?;
        self.set_ocp(120)?;
        self.set_lna(LnaGain::G1, true)?;

        self.write_register(Register::FifoTxBaseAddr, 0x00)?;
        self.write_register(Register::FifoRxBaseAddr, 0x00)?;

        // ModemConfig1: bandwidth | coding rate | header mode
        let mc1 = ((config.bandwidth as u8) << 4)
            | ((config.coding_rate as u8) << 1)
            | if config.header_mode == HeaderMode::Implicit {
                MODEM_CONFIG1_IMPLICIT_HEADER
            } else {
                0
            };
        self.write_register(Register::ModemConfig1, mc1)?;

        // SX1276 errata 2.1 — Sensitivity optimisation for 500 kHz bandwidth.
        //
        // With the default IfFreq1 = 0x20 the chip tunes its IF to ~500 kHz,
        // placing every received signal at (or beyond) the edge of the baseband
        // filter.  The resulting attenuation is 20+ dB for *all* signal levels,
        // not just weak ones — hence the badly low RSSI even at close range.
        // HighBwOptimize1/2 must also be changed for 500 kHz on the HF port.
        // For all other bandwidths, restore the reset defaults so that a later
        // reconfigure to a narrower BW doesn't inherit stale values.
        //
        // Reference: Semtech SX1276 errata note, §2.1.
        if config.bandwidth == Bandwidth::Bw500kHz && config.frequency > 525_000_000 {
            self.write_register(Register::IfFreq1, 0x00)?;
            self.write_register(Register::IfFreq2, 0x00)?;
            self.write_register(Register::HighBwOptimize1, 0x02)?;
            self.write_register(Register::HighBwOptimize2, 0x64)?;
        } else {
            self.write_register(Register::IfFreq1, 0x20)?;
            self.write_register(Register::IfFreq2, 0x00)?;
            self.write_register(Register::HighBwOptimize1, 0x03)?;
            self.write_register(Register::HighBwOptimize2, 0x65)?;
        }

        // ModemConfig2: SF | CRC
        let mc2 = ((config.spreading_factor as u8) << 4)
            | if config.crc_enabled {
                MODEM_CONFIG2_RX_PAYLOAD_CRC_ON
            } else {
                0
            };
        self.write_register(Register::ModemConfig2, mc2)?;

        // ModemConfig3: LDR optimize | AGC
        let ldro = config.should_use_low_data_rate_optimize();
        let mc3 = if ldro {
            MODEM_CONFIG3_LOW_DATA_RATE_OPTIMIZE
        } else {
            0
        } | if config.agc_auto_on {
            MODEM_CONFIG3_AGC_AUTO_ON
        } else {
            0
        };
        self.write_register(Register::ModemConfig3, mc3)?;

        // SF6 special registers
        if config.spreading_factor == SpreadingFactor::Sf6 {
            self.write_register(Register::DetectOptimize, DETECT_OPTIMIZE_SF6)?;
            self.write_register(Register::DetectionThreshold, DETECTION_THRESHOLD_SF6)?;
        } else {
            self.write_register(Register::DetectOptimize, DETECT_OPTIMIZE_SF7_TO_SF12)?;
            self.write_register(
                Register::DetectionThreshold,
                DETECTION_THRESHOLD_SF7_TO_SF12,
            )?;
        }

        self.write_register(Register::PreambleMsb, (config.preamble_length >> 8) as u8)?;
        self.write_register(Register::PreambleLsb, config.preamble_length as u8)?;
        self.write_register(Register::SyncWord, config.sync_word)?;

        // IQ inversion
        if config.invert_iq {
            let iq = self.read_register(Register::InvertIq)?;
            self.write_register(Register::InvertIq, iq | 0x40)?;
            self.write_register(Register::InvertIq2, 0x19)?;
        } else {
            let iq = self.read_register(Register::InvertIq)?;
            self.write_register(Register::InvertIq, iq & !0x40)?;
            self.write_register(Register::InvertIq2, 0x1D)?;
        }

        if config.header_mode == HeaderMode::Implicit {
            self.write_register(
                Register::PayloadLength,
                config.implicit_header_payload_length,
            )?;
        }
        self.write_register(Register::MaxPayloadLength, 255)?;

        log::info!(
            "RFM95 configured: freq={}Hz, bw={:?}, sf={:?}, cr={:?}",
            config.frequency,
            config.bandwidth,
            config.spreading_factor,
            config.coding_rate,
        );
        Ok(())
    }

    /// Set the carrier frequency in Hz.
    pub fn set_frequency(&mut self, freq_hz: u32) -> Result<()> {
        let frf: u64 = (freq_hz as u64 * (1u64 << 19)) / 32_000_000;
        self.write_register(Register::FrfMsb, ((frf >> 16) & 0xFF) as u8)?;
        self.write_register(Register::FrfMid, ((frf >> 8) & 0xFF) as u8)?;
        self.write_register(Register::FrfLsb, (frf & 0xFF) as u8)?;
        Ok(())
    }

    /// Read the current carrier frequency in Hz.
    pub fn get_frequency(&mut self) -> Result<u32> {
        let msb = self.read_register(Register::FrfMsb)? as u64;
        let mid = self.read_register(Register::FrfMid)? as u64;
        let lsb = self.read_register(Register::FrfLsb)? as u64;
        let frf = (msb << 16) | (mid << 8) | lsb;
        Ok(((frf * 32_000_000) / (1u64 << 19)) as u32)
    }

    /// Configure the power amplifier.
    pub fn set_pa_config(&mut self, pa: &PaConfig) -> Result<()> {
        match pa.pa_select {
            PaSelect::PaBoost => {
                let power = pa.output_power.clamp(2, 20);
                if power > 17 {
                    self.write_register(Register::PaDac, PA_DAC_BOOST)?;
                    self.set_ocp(140)?;
                    let val = PA_CONFIG_PA_SELECT_BOOST | ((power - 5).clamp(0, 15) as u8);
                    self.write_register(Register::PaConfig, val)?;
                } else {
                    self.write_register(Register::PaDac, PA_DAC_DEFAULT)?;
                    let val = PA_CONFIG_PA_SELECT_BOOST | ((power - 2).clamp(0, 15) as u8);
                    self.write_register(Register::PaConfig, val)?;
                }
            }
            PaSelect::Rfo => {
                self.write_register(Register::PaDac, PA_DAC_DEFAULT)?;
                let power = pa.output_power.clamp(-4, 15);
                let val = 0x70 | (power.clamp(0, 15) as u8);
                self.write_register(Register::PaConfig, val)?;
            }
        }
        Ok(())
    }

    /// Configure over-current protection (mA).
    pub fn set_ocp(&mut self, ma: u8) -> Result<()> {
        let trim = if ma <= 120 {
            (ma - 45) / 5
        } else if ma <= 240 {
            (ma + 30) / 10
        } else {
            27
        };
        self.write_register(Register::Ocp, OCP_ON | (trim & 0x1F))
    }

    /// Configure the LNA.
    pub fn set_lna(&mut self, gain: LnaGain, boost_hf: bool) -> Result<()> {
        let val = ((gain as u8) << 5) | if boost_hf { LNA_BOOST_HF_ON } else { 0 };
        self.write_register(Register::Lna, val)
    }

    // ── Transmit ─────────────────────────────────────────────────

    /// Transmit a LoRa packet. Blocks until done (10 s timeout).
    pub fn transmit(&mut self, payload: &[u8]) -> Result<()> {
        self.transmit_with_timeout(payload, Duration::from_secs(10))
    }

    /// Transmit with a configurable timeout.
    pub fn transmit_with_timeout(&mut self, payload: &[u8], timeout: Duration) -> Result<()> {
        if payload.len() > FIFO_SIZE as usize {
            return Err(Rfm95Error::PayloadTooLarge {
                size: payload.len(),
                max: FIFO_SIZE as usize,
            });
        }
        self.set_mode(OperatingMode::Standby)?;
        self.write_register(Register::FifoAddrPtr, 0x00)?;
        self.write_register(Register::PayloadLength, payload.len() as u8)?;
        self.write_burst(Register::Fifo, payload)?;
        self.write_register(Register::IrqFlags, 0xFF)?;
        self.set_mode(OperatingMode::Tx)?;

        let start = Instant::now();
        loop {
            let flags = IrqFlags::from_bits_truncate(self.read_register(Register::IrqFlags)?);
            if flags.contains(IrqFlags::TX_DONE) {
                break;
            }
            if start.elapsed() > timeout {
                self.set_mode(OperatingMode::Standby)?;
                return Err(Rfm95Error::Timeout(timeout));
            }
            thread::sleep(Duration::from_millis(1));
        }
        self.write_register(Register::IrqFlags, 0xFF)?;
        self.set_mode(OperatingMode::Standby)?;
        log::debug!("TX complete, {} bytes", payload.len());
        Ok(())
    }

    // ── Receive ──────────────────────────────────────────────────

    /// Blocking single-packet receive with timeout.
    pub fn receive(&mut self, timeout: Duration) -> Result<ReceivedPacket> {
        self.set_mode(OperatingMode::Standby)?;
        self.write_register(Register::FifoAddrPtr, 0x00)?;
        self.write_register(Register::IrqFlags, 0xFF)?;
        self.set_mode(OperatingMode::RxSingle)?;

        let start = Instant::now();
        loop {
            let flags = IrqFlags::from_bits_truncate(self.read_register(Register::IrqFlags)?);
            if flags.contains(IrqFlags::RX_DONE) {
                if flags.contains(IrqFlags::PAYLOAD_CRC_ERROR) {
                    self.write_register(Register::IrqFlags, 0xFF)?;
                    self.set_mode(OperatingMode::Standby)?;
                    return Err(Rfm95Error::CrcError);
                }
                return self.read_packet();
            }
            if flags.contains(IrqFlags::RX_TIMEOUT) || start.elapsed() > timeout {
                self.write_register(Register::IrqFlags, 0xFF)?;
                self.set_mode(OperatingMode::Standby)?;
                return Err(Rfm95Error::Timeout(timeout));
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    /// Start continuous receive. Poll with [`poll_receive`](Self::poll_receive).
    pub fn start_receive_continuous(&mut self) -> Result<()> {
        self.set_mode(OperatingMode::Standby)?;
        self.write_register(Register::FifoAddrPtr, 0x00)?;
        self.write_register(Register::IrqFlags, 0xFF)?;
        self.set_mode(OperatingMode::RxContinuous)?;
        log::debug!("Continuous RX started");
        Ok(())
    }

    /// Non-blocking poll for a received packet in continuous RX mode.
    pub fn poll_receive(&mut self) -> Result<Option<ReceivedPacket>> {
        let flags = IrqFlags::from_bits_truncate(self.read_register(Register::IrqFlags)?);
        if flags.contains(IrqFlags::RX_DONE) {
            if flags.contains(IrqFlags::PAYLOAD_CRC_ERROR) {
                self.write_register(Register::IrqFlags, 0xFF)?;
                return Err(Rfm95Error::CrcError);
            }
            let pkt = self.read_packet()?;
            self.write_register(Register::FifoAddrPtr, 0x00)?;
            return Ok(Some(pkt));
        }
        Ok(None)
    }

    fn read_packet(&mut self) -> Result<ReceivedPacket> {
        let nb_bytes = self.read_register(Register::RxNbBytes)?;
        let current_addr = self.read_register(Register::FifoRxCurrentAddr)?;
        self.write_register(Register::FifoAddrPtr, current_addr)?;
        let payload = self.read_burst(Register::Fifo, nb_bytes as usize)?;
        let snr = self.get_packet_snr()?;
        let rssi = self.get_packet_rssi(snr)?;
        self.write_register(Register::IrqFlags, 0xFF)?;
        log::debug!(
            "RX: {} bytes, RSSI={} dBm, SNR={:.1} dB",
            payload.len(),
            rssi,
            snr
        );
        Ok(ReceivedPacket { payload, rssi, snr })
    }

    // ── Signal quality ───────────────────────────────────────────

    /// Current RSSI in dBm (while in receive mode).
    pub fn get_rssi(&mut self) -> Result<i16> {
        let raw = self.read_register(Register::RssiValue)?;
        let offset: i16 = if self.config.frequency < 779_000_000 {
            -164
        } else {
            -157
        };
        Ok(raw as i16 + offset)
    }

    /// SNR of the last received packet (dB).
    pub fn get_packet_snr(&mut self) -> Result<f32> {
        let raw = self.read_register(Register::PktSnrValue)? as i8;
        Ok(raw as f32 * 0.25)
    }

    /// RSSI of the last received packet (dBm).
    pub fn get_packet_rssi(&mut self, snr: f32) -> Result<i16> {
        let raw = self.read_register(Register::PktRssiValue)?;
        let offset: i16 = if self.config.frequency < 779_000_000 {
            -164
        } else {
            -157
        };
        let rssi = if snr >= 0.0 {
            offset + (raw as f32 * 16.0 / 15.0) as i16
        } else {
            offset + raw as i16 + snr as i16
        };
        Ok(rssi)
    }

    // ── CAD ──────────────────────────────────────────────────────

    /// Channel Activity Detection. Returns `true` if LoRa signal detected.
    pub fn cad(&mut self, timeout: Duration) -> Result<bool> {
        self.set_mode(OperatingMode::Standby)?;
        self.write_register(Register::IrqFlags, 0xFF)?;
        self.set_mode(OperatingMode::Cad)?;

        let start = Instant::now();
        loop {
            let flags = IrqFlags::from_bits_truncate(self.read_register(Register::IrqFlags)?);
            if flags.contains(IrqFlags::CAD_DONE) {
                let detected = flags.contains(IrqFlags::CAD_DETECTED);
                self.write_register(Register::IrqFlags, 0xFF)?;
                self.set_mode(OperatingMode::Standby)?;
                return Ok(detected);
            }
            if start.elapsed() > timeout {
                self.set_mode(OperatingMode::Standby)?;
                return Err(Rfm95Error::Timeout(timeout));
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    // ── IRQ helpers ──────────────────────────────────────────────

    /// Read current interrupt flags.
    pub fn get_irq_flags(&mut self) -> Result<IrqFlags> {
        Ok(IrqFlags::from_bits_truncate(
            self.read_register(Register::IrqFlags)?,
        ))
    }

    /// Clear all interrupt flags.
    pub fn clear_irq_flags(&mut self) -> Result<()> {
        self.write_register(Register::IrqFlags, 0xFF)
    }

    /// Set the IRQ flag mask (1 = masked / disabled).
    pub fn set_irq_mask(&mut self, mask: IrqFlags) -> Result<()> {
        self.write_register(Register::IrqFlagsMask, mask.bits())
    }

    /// Read the DIO0 pin. Returns `None` if unconfigured.
    pub fn read_dio0(&self) -> Result<Option<bool>> {
        match &self.dio0_line {
            Some(line) => Ok(Some(line.get_value()? != 0)),
            None => Ok(None),
        }
    }

    // ── Misc ─────────────────────────────────────────────────────

    /// Get a reference to the current configuration.
    pub fn current_config(&self) -> &LoraConfig {
        &self.config
    }

    /// Read the modem status register.
    pub fn get_modem_status(&mut self) -> Result<u8> {
        self.read_register(Register::ModemStat)
    }

    /// Set the LoRa sync word.
    pub fn set_sync_word(&mut self, word: u8) -> Result<()> {
        self.write_register(Register::SyncWord, word)
    }

    /// Read any register by raw address (for debugging).
    pub fn debug_read_register(&mut self, addr: u8) -> Result<u8> {
        let tx = [addr & 0x7F, 0x00];
        let mut rx = [0u8; 2];
        let mut xfer = SpidevTransfer::read_write(&tx, &mut rx);
        self.spi.transfer(&mut xfer)?;
        Ok(rx[1])
    }
}
