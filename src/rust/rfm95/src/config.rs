//! Configuration types for the RFM95W / SX1276 LoRa radio.

/// Operating mode of the radio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OperatingMode {
    Sleep = 0x00,
    Standby = 0x01,
    FrequencySynthesisTx = 0x02,
    Tx = 0x03,
    FrequencySynthesisRx = 0x04,
    RxContinuous = 0x05,
    RxSingle = 0x06,
    Cad = 0x07,
}

/// LoRa signal bandwidth.
///
/// Wider bandwidths give higher data rates but lower sensitivity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Bandwidth {
    Bw7_8kHz = 0x00,
    Bw10_4kHz = 0x01,
    Bw15_6kHz = 0x02,
    Bw20_8kHz = 0x03,
    Bw31_25kHz = 0x04,
    Bw41_7kHz = 0x05,
    Bw62_5kHz = 0x06,
    /// 125 kHz — most common default.
    Bw125kHz = 0x07,
    Bw250kHz = 0x08,
    Bw500kHz = 0x09,
}

/// LoRa error coding rate. Higher rates = more redundancy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CodingRate {
    Cr4_5 = 0x01,
    Cr4_6 = 0x02,
    Cr4_7 = 0x03,
    Cr4_8 = 0x04,
}

/// LoRa spreading factor. Higher SF = longer range, lower data rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SpreadingFactor {
    /// Requires implicit header mode.
    Sf6 = 6,
    /// Default — good balance of range and throughput.
    Sf7 = 7,
    Sf8 = 8,
    Sf9 = 9,
    Sf10 = 10,
    Sf11 = 11,
    /// Maximum range, minimum data rate.
    Sf12 = 12,
}

/// LoRa packet header mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderMode {
    /// Payload length, coding rate and CRC presence are in the header.
    Explicit,
    /// No header. Receiver must know parameters in advance. Required for SF6.
    Implicit,
}

/// PA output pin selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaSelect {
    /// RFO pin. Output power: -4 to +15 dBm.
    Rfo,
    /// PA_BOOST pin. Output power: +2 to +17 dBm (+20 dBm with PA_DAC).
    /// Connected on most RFM95 modules.
    PaBoost,
}

/// LNA gain setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LnaGain {
    /// Highest gain (default for RX).
    G1 = 0x01,
    G2 = 0x02,
    G3 = 0x03,
    G4 = 0x04,
    G5 = 0x05,
    /// Lowest gain.
    G6 = 0x06,
}

/// Power amplifier configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaConfig {
    /// Which output pin to use.
    pub pa_select: PaSelect,
    /// Output power in dBm.
    pub output_power: i8,
}

impl Default for PaConfig {
    fn default() -> Self {
        Self {
            pa_select: PaSelect::PaBoost,
            output_power: 17,
        }
    }
}

/// Complete LoRa radio configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoraConfig {
    /// Carrier frequency in Hz (default: 915 MHz).
    pub frequency: u32,
    pub bandwidth: Bandwidth,
    pub spreading_factor: SpreadingFactor,
    pub coding_rate: CodingRate,
    pub header_mode: HeaderMode,
    /// Preamble length in symbols (6..=65535, default: 8).
    pub preamble_length: u16,
    /// Sync word. 0x12 = private, 0x34 = LoRaWAN public.
    pub sync_word: u8,
    pub crc_enabled: bool,
    /// Invert IQ signals (for certain network protocols).
    pub invert_iq: bool,
    pub pa_config: PaConfig,
    /// Enable automatic gain control.
    pub agc_auto_on: bool,
    /// Low data rate optimization override. `None` = auto-detect.
    pub low_data_rate_optimize: Option<bool>,
    /// Fixed payload length for implicit header mode (1..=255).
    pub implicit_header_payload_length: u8,
}

impl Default for LoraConfig {
    fn default() -> Self {
        Self {
            frequency: 915_000_000,
            bandwidth: Bandwidth::Bw125kHz,
            spreading_factor: SpreadingFactor::Sf7,
            coding_rate: CodingRate::Cr4_5,
            header_mode: HeaderMode::Explicit,
            preamble_length: 8,
            sync_word: 0x12,
            crc_enabled: true,
            invert_iq: false,
            pa_config: PaConfig::default(),
            agc_auto_on: true,
            low_data_rate_optimize: None,
            implicit_header_payload_length: 255,
        }
    }
}

impl LoraConfig {
    /// Preset for 433 MHz band.
    pub fn with_frequency_433() -> Self {
        Self {
            frequency: 433_000_000,
            ..Default::default()
        }
    }

    /// Preset for 868 MHz band (EU).
    pub fn with_frequency_868() -> Self {
        Self {
            frequency: 868_000_000,
            ..Default::default()
        }
    }

    /// Preset for 915 MHz band (US/AU).
    pub fn with_frequency_915() -> Self {
        Self::default()
    }

    /// Long-range, low-bitrate preset.
    pub fn long_range() -> Self {
        Self {
            spreading_factor: SpreadingFactor::Sf12,
            bandwidth: Bandwidth::Bw125kHz,
            coding_rate: CodingRate::Cr4_8,
            ..Default::default()
        }
    }

    /// Fast, short-range preset.
    pub fn fast() -> Self {
        Self {
            spreading_factor: SpreadingFactor::Sf7,
            bandwidth: Bandwidth::Bw500kHz,
            coding_rate: CodingRate::Cr4_5,
            ..Default::default()
        }
    }

    /// Whether low data rate optimization should be enabled.
    /// Auto-calculates based on symbol duration when `low_data_rate_optimize` is `None`.
    pub fn should_use_low_data_rate_optimize(&self) -> bool {
        if let Some(v) = self.low_data_rate_optimize {
            return v;
        }
        let bw_hz: u32 = match self.bandwidth {
            Bandwidth::Bw7_8kHz => 7_800,
            Bandwidth::Bw10_4kHz => 10_400,
            Bandwidth::Bw15_6kHz => 15_600,
            Bandwidth::Bw20_8kHz => 20_800,
            Bandwidth::Bw31_25kHz => 31_250,
            Bandwidth::Bw41_7kHz => 41_700,
            Bandwidth::Bw62_5kHz => 62_500,
            Bandwidth::Bw125kHz => 125_000,
            Bandwidth::Bw250kHz => 250_000,
            Bandwidth::Bw500kHz => 500_000,
        };
        // Symbol duration = 2^SF / BW. Mandatory when > 16 ms.
        let symbol_duration_us = (1u64 << self.spreading_factor as u32) * 1_000_000 / bw_hz as u64;
        symbol_duration_us > 16_000
    }
}
