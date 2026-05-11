//! Register map for the Semtech SX1276 / HopeRF RFM95W LoRa transceiver.
//!
//! Reference: SX1276/77/78/79 Datasheet, Revision 7, Semtech.

/// Every register in the SX1276 relevant to LoRa operating mode.
///
/// The discriminant is the 7-bit register address (MSB is the SPI R/W bit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Register {
    Fifo = 0x00,
    OpMode = 0x01,
    FrfMsb = 0x06,
    FrfMid = 0x07,
    FrfLsb = 0x08,
    PaConfig = 0x09,
    PaRamp = 0x0A,
    Ocp = 0x0B,
    Lna = 0x0C,
    FifoAddrPtr = 0x0D,
    FifoTxBaseAddr = 0x0E,
    FifoRxBaseAddr = 0x0F,
    FifoRxCurrentAddr = 0x10,
    IrqFlagsMask = 0x11,
    IrqFlags = 0x12,
    RxNbBytes = 0x13,
    RxHeaderCntValueMsb = 0x14,
    RxHeaderCntValueLsb = 0x15,
    RxPacketCntValueMsb = 0x16,
    RxPacketCntValueLsb = 0x17,
    ModemStat = 0x18,
    PktSnrValue = 0x19,
    PktRssiValue = 0x1A,
    RssiValue = 0x1B,
    HopChannel = 0x1C,
    ModemConfig1 = 0x1D,
    ModemConfig2 = 0x1E,
    SymbTimeoutLsb = 0x1F,
    PreambleMsb = 0x20,
    PreambleLsb = 0x21,
    PayloadLength = 0x22,
    MaxPayloadLength = 0x23,
    HopPeriod = 0x24,
    FifoRxByteAddr = 0x25,
    ModemConfig3 = 0x26,
    PpmCorrection = 0x27,
    FeiMsb = 0x28,
    FeiMid = 0x29,
    FeiLsb = 0x2A,
    RssiWideband = 0x2C,
    IfFreq1 = 0x2F,
    IfFreq2 = 0x30,
    DetectOptimize = 0x31,
    InvertIq = 0x33,
    HighBwOptimize1 = 0x36,
    DetectionThreshold = 0x37,
    SyncWord = 0x39,
    HighBwOptimize2 = 0x3A,
    InvertIq2 = 0x3B,
    DioMapping1 = 0x40,
    DioMapping2 = 0x41,
    Version = 0x42,
    Tcxo = 0x4B,
    PaDac = 0x4D,
    FormerTemp = 0x5B,
    AgcRef = 0x61,
    AgcThresh1 = 0x62,
    AgcThresh2 = 0x63,
    AgcThresh3 = 0x64,
}

impl Register {
    /// Return the raw 7-bit register address.
    #[inline]
    pub fn addr(self) -> u8 {
        self as u8
    }
}

// ── IRQ flag bits (LoRa mode) ────────────────────────────────────────

bitflags::bitflags! {
    /// Bit flags for the `IrqFlags` register (0x12).
    /// Writing a 1 to a bit clears that interrupt.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IrqFlags: u8 {
        const CAD_DETECTED        = 0x01;
        const FHSS_CHANGE_CHANNEL = 0x02;
        const CAD_DONE            = 0x04;
        const TX_DONE             = 0x08;
        const VALID_HEADER        = 0x10;
        const PAYLOAD_CRC_ERROR   = 0x20;
        const RX_DONE             = 0x40;
        const RX_TIMEOUT          = 0x80;
    }
}

// ── OpMode register ──────────────────────────────────────────────────

/// Enable LoRa mode (bit 7).
pub const OPMODE_LONG_RANGE_MODE: u8 = 0x80;
/// Access shared registers between FSK and LoRa (bit 6).
pub const OPMODE_ACCESS_SHARED_REG: u8 = 0x40;
/// Low-frequency mode enable (bit 3).
pub const OPMODE_LOW_FREQUENCY_MODE: u8 = 0x08;
/// Mask for operating-mode bits [2:0].
pub const OPMODE_MODE_MASK: u8 = 0x07;

// ── PaConfig register ────────────────────────────────────────────────

/// Select PA_BOOST output pin (bit 7).
pub const PA_CONFIG_PA_SELECT_BOOST: u8 = 0x80;
/// MaxPower field mask [6:4].
pub const PA_CONFIG_MAX_POWER_MASK: u8 = 0x70;
/// OutputPower field mask [3:0].
pub const PA_CONFIG_OUTPUT_POWER_MASK: u8 = 0x0F;

// ── PaDac ────────────────────────────────────────────────────────────

/// Default PA DAC (normal power).
pub const PA_DAC_DEFAULT: u8 = 0x84;
/// PA DAC for +20 dBm on PA_BOOST.
pub const PA_DAC_BOOST: u8 = 0x87;

// ── OCP ──────────────────────────────────────────────────────────────

/// Over-current protection enable (bit 5).
pub const OCP_ON: u8 = 0x20;

// ── LNA ──────────────────────────────────────────────────────────────

/// LNA gain mask [7:5].
pub const LNA_GAIN_MASK: u8 = 0xE0;
/// 150% LNA boost on HF port (bits [1:0] = 0b11).
pub const LNA_BOOST_HF_ON: u8 = 0x03;

// ── ModemConfig1 ─────────────────────────────────────────────────────

/// Bandwidth mask [7:4].
pub const MODEM_CONFIG1_BW_MASK: u8 = 0xF0;
/// Coding rate mask [3:1].
pub const MODEM_CONFIG1_CR_MASK: u8 = 0x0E;
/// Implicit header mode (bit 0).
pub const MODEM_CONFIG1_IMPLICIT_HEADER: u8 = 0x01;

// ── ModemConfig2 ─────────────────────────────────────────────────────

/// Spreading factor mask [7:4].
pub const MODEM_CONFIG2_SF_MASK: u8 = 0xF0;
/// TX continuous mode (bit 3).
pub const MODEM_CONFIG2_TX_CONTINUOUS: u8 = 0x08;
/// CRC on payload (bit 2).
pub const MODEM_CONFIG2_RX_PAYLOAD_CRC_ON: u8 = 0x04;
/// Symbol timeout MSB mask [1:0].
pub const MODEM_CONFIG2_SYMB_TIMEOUT_MSB_MASK: u8 = 0x03;

// ── ModemConfig3 ─────────────────────────────────────────────────────

/// Low data rate optimization (mandatory when symbol time > 16 ms).
pub const MODEM_CONFIG3_LOW_DATA_RATE_OPTIMIZE: u8 = 0x08;
/// AGC auto-on.
pub const MODEM_CONFIG3_AGC_AUTO_ON: u8 = 0x04;

// ── DetectOptimize / DetectionThreshold ──────────────────────────────

pub const DETECT_OPTIMIZE_SF6: u8 = 0x05;
pub const DETECT_OPTIMIZE_SF7_TO_SF12: u8 = 0x03;
pub const DETECTION_THRESHOLD_SF6: u8 = 0x0C;
pub const DETECTION_THRESHOLD_SF7_TO_SF12: u8 = 0x0A;

// ── Sync word ────────────────────────────────────────────────────────

/// Default LoRa sync word (private networks).
pub const LORA_SYNC_WORD_DEFAULT: u8 = 0x12;
/// LoRaWAN public-network sync word.
pub const LORA_SYNC_WORD_LORAWAN: u8 = 0x34;

// ── Misc constants ───────────────────────────────────────────────────

/// Expected chip version for SX1276/RFM95W.
pub const EXPECTED_VERSION: u8 = 0x12;
/// OR with register address for SPI write.
pub const SPI_WRITE_MASK: u8 = 0x80;
/// FIFO buffer size (256 bytes, indices 0-255).
pub const FIFO_SIZE: u8 = 255;
