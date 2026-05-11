//! Shared telemetry protocol for the Sirius ↔ Sopdet LoRa radio link.
//!
//! This crate is intentionally dependency-free so it can be imported by both
//! the flight computer (`sirius`) and the ground station (`sopdet`) without
//! pulling in any hardware-specific code.
//!
//! # Packet types
//!
//! | Byte 0 | Direction       | Purpose                                    |
//! |--------|-----------------|--------------------------------------------|
//! | 0x01   | Rocket → Ground | [`DownlinkPacket`]: telemetry snapshot     |
//! | 0x02   | Ground → Rocket | [`UplinkPacket`]: commands + inline RTK    |
//! | 0x03   | Ground → Rocket | [`RtcmFragment`]: one slice of a large RTK batch |
//!
//! All multi-byte integers are **little-endian**.
//!
//! # Downlink layout (43 bytes, fixed)
//!
//! ```text
//!  [0]       type            u8  = 0x01
//!  [1-2]     sequence_num    u16
//!  [3-6]     timestamp_ms    u32  (ms since rocket boot)
//!  [7-10]    altitude_m      f32  (AGL, metres, Kalman-filtered)
//!  [11-14]   velocity_mps    f32  (vertical, m/s, + = up)
//!  [15-18]   accel_z_gs      f32  (calibrated body-frame Z, g)
//!  [19-26]   gps_lat         f64  (decimal degrees)
//!  [27-34]   gps_lon         f64  (decimal degrees)
//!  [35-38]   gps_alt_m       f32  (GPS altitude MSL, metres)
//!  [39]      rtk_fix         u8   (RtkFixType discriminant)
//!  [40]      pyro_deployed   u8   (0/1)
//!  [41]      pyro_continuity u8   (0/1)
//!  [42]      flight_state    u8   (0=Standby … 4=Landed)
//! ```
//!
//! # Uplink layout (≥ 3 bytes, variable)
//!
//! ```text
//!  [0]       type            u8  = 0x02
//!  [1]       command         u8  (GroundCommand discriminant)
//!  [2]       rtk_data_len    u8  (0 – 252)
//!  [3..]     rtk_data        bytes
//! ```
//!
//! # RTCM fragment layout (≥ 5 bytes, variable)
//!
//! ```text
//!  [0]       type            u8  = 0x03
//!  [1]       session_id      u8  (incremented each new RTCM epoch by sopdet)
//!  [2]       frag_index      u8  (0-based)
//!  [3]       total_frags     u8
//!  [4]       data_len        u8  (0 – 250)
//!  [5..]     data            bytes
//! ```

// ── Discriminants ─────────────────────────────────────────────────────────────

pub const DOWNLINK_TYPE: u8 = 0x01;
pub const UPLINK_TYPE: u8 = 0x02;
pub const FRAG_TYPE: u8 = 0x03;
/// Debug telemetry packet (Rocket → Ground).  Only transmitted when the
/// ground station has activated debug mode via [`GroundCommand::EnableDebugTelemetry`].
pub const DEBUG_TYPE: u8 = 0x04;

/// Maximum RTCM data bytes in a single non-fragmented uplink
/// (255 total − 3 header bytes).
pub const MAX_UPLINK_RTK: usize = 252;

/// Maximum RTCM data bytes in a single fragment (255 total − 5 header bytes).
pub const MAX_FRAG_DATA: usize = 250;

// ── RtkFixType ────────────────────────────────────────────────────────────────

/// GPS/RTK fix quality, matching the GGA sentence quality indicator values
/// (0–6) directly.
///
/// All variants defined in the NMEA 0183 GGA specification are represented so
/// that the telemetry log preserves the full fix-type information rather than
/// collapsing it into a three-state enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum RtkFixType {
    /// No valid position fix.
    #[default]
    NoFix = 0,
    /// Standard GPS SPS mode — fix valid.
    GpsFix = 1,
    /// Differential GPS (DGPS / SBAS) — fix valid.
    DgpsFix = 2,
    /// GPS PPS mode — fix valid.
    PpsFix = 3,
    /// Real-Time Kinematic, fixed integers — highest accuracy.
    RtkFixed = 4,
    /// Real-Time Kinematic, float solution.
    RtkFloat = 5,
    /// Dead-reckoning mode.
    DeadReckoning = 6,
}

impl RtkFixType {
    /// Lossless conversion from the raw GGA quality byte.
    /// Unknown values map to [`RtkFixType::NoFix`].
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => RtkFixType::GpsFix,
            2 => RtkFixType::DgpsFix,
            3 => RtkFixType::PpsFix,
            4 => RtkFixType::RtkFixed,
            5 => RtkFixType::RtkFloat,
            6 => RtkFixType::DeadReckoning,
            _ => RtkFixType::NoFix,
        }
    }

    /// Raw wire byte (same encoding as the GGA quality field).
    #[inline]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Short human-readable label for CSV logs and display.
    pub fn as_str(self) -> &'static str {
        match self {
            RtkFixType::NoFix => "NoFix",
            RtkFixType::GpsFix => "GPS",
            RtkFixType::DgpsFix => "DGPS",
            RtkFixType::PpsFix => "PPS",
            RtkFixType::RtkFixed => "RTK-Fixed",
            RtkFixType::RtkFloat => "RTK-Float",
            RtkFixType::DeadReckoning => "DeadReckoning",
        }
    }
}

impl core::fmt::Display for RtkFixType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── GroundCommand ─────────────────────────────────────────────────────────────

/// Commands that the ground station can send to the rocket via the uplink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GroundCommand {
    /// No command; uplink carries only RTK data.
    None = 0,
    /// Activate the emergency locator buzzer on the rocket.
    EmergencyLocate = 1,
    /// Fire the ejection charge immediately, regardless of flight state.
    DeployEjectionCharge = 2,
    /// Deactivate the emergency locator buzzer on the rocket.
    EmergencyLocateOff = 3,
    /// Ask the rocket to send per-constellation GPS SNR debug packets.
    EnableDebugTelemetry = 4,
    /// Stop sending debug packets.
    DisableDebugTelemetry = 5,
}

impl GroundCommand {
    /// Decode from a raw byte; unknown values map to [`GroundCommand::None`].
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => GroundCommand::EmergencyLocate,
            2 => GroundCommand::DeployEjectionCharge,
            3 => GroundCommand::EmergencyLocateOff,
            4 => GroundCommand::EnableDebugTelemetry,
            5 => GroundCommand::DisableDebugTelemetry,
            _ => GroundCommand::None,
        }
    }

    #[inline]
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl core::fmt::Display for GroundCommand {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            GroundCommand::None => f.write_str("None"),
            GroundCommand::EmergencyLocate => f.write_str("EmergencyLocate"),
            GroundCommand::DeployEjectionCharge => f.write_str("DeployEjectionCharge"),
            GroundCommand::EmergencyLocateOff => f.write_str("EmergencyLocateOff"),
            GroundCommand::EnableDebugTelemetry => f.write_str("EnableDebugTelemetry"),
            GroundCommand::DisableDebugTelemetry => f.write_str("DisableDebugTelemetry"),
        }
    }
}

// ── DownlinkPacket ────────────────────────────────────────────────────────────

/// Telemetry snapshot transmitted from the rocket to the ground station.
///
/// Fixed size: [`DownlinkPacket::SIZE`] bytes.  Use [`serialize`](Self::serialize)
/// and [`deserialize`](Self::deserialize) for wire encoding.
#[derive(Debug, Clone)]
pub struct DownlinkPacket {
    /// Rolling TX sequence counter — wraps at `u16::MAX`.
    /// Allows the ground station to detect dropped packets (sequence gaps).
    pub sequence_num: u16,
    /// Milliseconds since rocket boot.
    pub timestamp_ms: u32,
    /// Altitude above ground level (metres), Kalman-filtered.
    pub altitude_m: f32,
    /// Vertical velocity (m/s, positive = upward), Kalman-filtered.
    pub velocity_mps: f32,
    /// Calibrated body-frame Z-axis acceleration (g).
    pub accel_z_gs: f32,
    /// GPS latitude (decimal degrees, positive = North).
    pub gps_lat: f64,
    /// GPS longitude (decimal degrees, positive = East).
    pub gps_lon: f64,
    /// GPS altitude above mean sea level (metres).
    pub gps_alt_m: f32,
    /// Full GPS/RTK fix type.
    pub rtk_fix: RtkFixType,
    /// Whether the ejection charge has been fired.
    pub pyro_deployed: bool,
    /// Continuity present on pyro channel.
    pub pyro_continuity: bool,
    /// Current flight state (0 = Standby, 1 = MotorBurn, 2 = Coast,
    /// 3 = Freefall, 4 = Landed).
    pub flight_state: u8,
    /// Average GPS signal-to-noise ratio across all tracked satellites (dB-Hz).
    /// Computed from NMEA GSV sentences. Zero if no GSV data available.
    pub gps_snr: u8,
}

impl DownlinkPacket {
    /// Wire size of a serialised downlink packet (bytes).
    pub const SIZE: usize = 44;

    /// Serialise to a fixed-size byte array.
    pub fn serialize(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = DOWNLINK_TYPE;
        b[1..3].copy_from_slice(&self.sequence_num.to_le_bytes());
        b[3..7].copy_from_slice(&self.timestamp_ms.to_le_bytes());
        b[7..11].copy_from_slice(&self.altitude_m.to_le_bytes());
        b[11..15].copy_from_slice(&self.velocity_mps.to_le_bytes());
        b[15..19].copy_from_slice(&self.accel_z_gs.to_le_bytes());
        b[19..27].copy_from_slice(&self.gps_lat.to_le_bytes());
        b[27..35].copy_from_slice(&self.gps_lon.to_le_bytes());
        b[35..39].copy_from_slice(&self.gps_alt_m.to_le_bytes());
        b[39] = self.rtk_fix.as_u8();
        b[40] = self.pyro_deployed as u8;
        b[41] = self.pyro_continuity as u8;
        b[42] = self.flight_state;
        b[43] = self.gps_snr;
        b
    }

    /// Deserialise from a byte slice.  Returns `None` if the slice is shorter
    /// than [`SIZE`](Self::SIZE) bytes or the type byte is wrong.
    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE || bytes[0] != DOWNLINK_TYPE {
            return None;
        }
        Some(DownlinkPacket {
            sequence_num: u16::from_le_bytes([bytes[1], bytes[2]]),
            timestamp_ms: u32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]),
            altitude_m: f32::from_le_bytes([bytes[7], bytes[8], bytes[9], bytes[10]]),
            velocity_mps: f32::from_le_bytes([bytes[11], bytes[12], bytes[13], bytes[14]]),
            accel_z_gs: f32::from_le_bytes([bytes[15], bytes[16], bytes[17], bytes[18]]),
            gps_lat: f64::from_le_bytes([
                bytes[19], bytes[20], bytes[21], bytes[22], bytes[23], bytes[24], bytes[25],
                bytes[26],
            ]),
            gps_lon: f64::from_le_bytes([
                bytes[27], bytes[28], bytes[29], bytes[30], bytes[31], bytes[32], bytes[33],
                bytes[34],
            ]),
            gps_alt_m: f32::from_le_bytes([bytes[35], bytes[36], bytes[37], bytes[38]]),
            rtk_fix: RtkFixType::from_u8(bytes[39]),
            pyro_deployed: bytes[40] != 0,
            pyro_continuity: bytes[41] != 0,
            flight_state: bytes[42],
            gps_snr: bytes[43],
        })
    }
}

// ── DebugDownlinkPacket ────────────────────────────────────────────────────────

/// Per-constellation GPS SNR debug packet, sent by the rocket only while
/// [`GroundCommand::EnableDebugTelemetry`] is active.
///
/// # Wire layout (8 bytes, fixed)
/// ```text
///  [0]    type     u8 = 0x04
///  [1-2]  seq_num  u16  (matches the concurrent DownlinkPacket sequence)
///  [3]    gps      u8   GPS / NAVSTAR average SNR (dB-Hz)
///  [4]    glonass  u8   GLONASS
///  [5]    galileo  u8   Galileo
///  [6]    beidou   u8   BeiDou
///  [7]    qzss     u8   QZSS
/// ```
#[derive(Debug, Clone, Default)]
pub struct DebugDownlinkPacket {
    pub sequence_num: u16,
    pub gps: u8,
    pub glonass: u8,
    pub galileo: u8,
    pub beidou: u8,
    pub qzss: u8,
}

impl DebugDownlinkPacket {
    pub const SIZE: usize = 8;

    pub fn serialize(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = DEBUG_TYPE;
        b[1..3].copy_from_slice(&self.sequence_num.to_le_bytes());
        b[3] = self.gps;
        b[4] = self.glonass;
        b[5] = self.galileo;
        b[6] = self.beidou;
        b[7] = self.qzss;
        b
    }

    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE || bytes[0] != DEBUG_TYPE {
            return None;
        }
        Some(DebugDownlinkPacket {
            sequence_num: u16::from_le_bytes([bytes[1], bytes[2]]),
            gps: bytes[3],
            glonass: bytes[4],
            galileo: bytes[5],
            beidou: bytes[6],
            qzss: bytes[7],
        })
    }
}

// ── UplinkPacket ────────────────────────────────────────────────────────────
///
/// Carries an optional [`GroundCommand`] and/or a small inline RTCM correction
/// payload.  When the RTCM data for a single epoch is larger than
/// [`MAX_UPLINK_RTK`] bytes, sopdet should use [`RtcmFragment`] packets
/// instead and leave `rtk_data` empty here.
#[derive(Debug, Clone)]
pub struct UplinkPacket {
    /// Command to execute on the rocket (use [`GroundCommand::None`] if only
    /// sending RTK data).
    pub command: GroundCommand,
    /// Raw RTCM correction bytes.  Silently truncated to [`MAX_UPLINK_RTK`]
    /// bytes during serialisation.
    pub rtk_data: Vec<u8>,
}

impl UplinkPacket {
    /// Serialise to bytes.
    pub fn serialize(&self) -> Vec<u8> {
        let rtk = &self.rtk_data[..self.rtk_data.len().min(MAX_UPLINK_RTK)];
        let mut b = Vec::with_capacity(3 + rtk.len());
        b.push(UPLINK_TYPE);
        b.push(self.command.as_u8());
        b.push(rtk.len() as u8);
        b.extend_from_slice(rtk);
        b
    }

    /// Deserialise from a received byte slice.
    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 3 || bytes[0] != UPLINK_TYPE {
            return None;
        }
        let rtk_len = bytes[2] as usize;
        if bytes.len() < 3 + rtk_len {
            return None;
        }
        Some(UplinkPacket {
            command: GroundCommand::from_u8(bytes[1]),
            rtk_data: bytes[3..3 + rtk_len].to_vec(),
        })
    }
}

// ── RtcmFragment ──────────────────────────────────────────────────────────────

/// One fragment of a large RTCM correction batch.
///
/// When the total RTCM data for a 1-second epoch exceeds [`MAX_UPLINK_RTK`]
/// bytes, sopdet splits it across multiple `RtcmFragment` packets.  Sirius
/// reassembles them using the `session_id` / `frag_index` / `total_frags`
/// fields before forwarding the complete buffer to the LC29H GPS module.
#[derive(Debug, Clone)]
pub struct RtcmFragment {
    /// Batch identifier — sopdet increments this for each new RTCM epoch.
    /// Allows sirius to detect when a new batch begins and discard any
    /// incomplete previous batch.
    pub session_id: u8,
    /// 0-based index of this fragment within the batch.
    pub frag_index: u8,
    /// Total number of fragments in this batch.
    pub total_frags: u8,
    /// Raw RTCM bytes for this slice.
    pub data: Vec<u8>,
}

impl RtcmFragment {
    /// Serialise to bytes.  `data` is silently truncated to [`MAX_FRAG_DATA`].
    pub fn serialize(&self) -> Vec<u8> {
        let data = &self.data[..self.data.len().min(MAX_FRAG_DATA)];
        let mut b = Vec::with_capacity(5 + data.len());
        b.push(FRAG_TYPE);
        b.push(self.session_id);
        b.push(self.frag_index);
        b.push(self.total_frags);
        b.push(data.len() as u8);
        b.extend_from_slice(data);
        b
    }

    /// Deserialise from a received byte slice.
    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 5 || bytes[0] != FRAG_TYPE {
            return None;
        }
        let data_len = bytes[4] as usize;
        if bytes.len() < 5 + data_len {
            return None;
        }
        Some(RtcmFragment {
            session_id: bytes[1],
            frag_index: bytes[2],
            total_frags: bytes[3],
            data: bytes[5..5 + data_len].to_vec(),
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── RtkFixType ────────────────────────────────────────────────────────────

    #[test]
    fn rtk_fix_roundtrip_all_variants() {
        for v in 0u8..=6 {
            assert_eq!(RtkFixType::from_u8(v).as_u8(), v);
        }
    }

    #[test]
    fn rtk_fix_unknown_maps_to_no_fix() {
        assert_eq!(RtkFixType::from_u8(7), RtkFixType::NoFix);
        assert_eq!(RtkFixType::from_u8(255), RtkFixType::NoFix);
    }

    #[test]
    fn rtk_fix_display_strings_are_non_empty() {
        let variants = [
            RtkFixType::NoFix,
            RtkFixType::GpsFix,
            RtkFixType::DgpsFix,
            RtkFixType::PpsFix,
            RtkFixType::RtkFixed,
            RtkFixType::RtkFloat,
            RtkFixType::DeadReckoning,
        ];
        for v in variants {
            assert!(!v.as_str().is_empty());
            assert!(!v.to_string().is_empty());
        }
    }

    // ── GroundCommand ─────────────────────────────────────────────────────────

    #[test]
    fn ground_command_roundtrip() {
        for v in 0u8..=2 {
            assert_eq!(GroundCommand::from_u8(v).as_u8(), v);
        }
    }

    #[test]
    fn ground_command_unknown_maps_to_none() {
        assert_eq!(GroundCommand::from_u8(99), GroundCommand::None);
    }

    // ── DownlinkPacket ────────────────────────────────────────────────────────

    #[test]
    fn downlink_serialise_correct_size() {
        let pkt = DownlinkPacket {
            sequence_num: 1,
            timestamp_ms: 5000,
            altitude_m: 1234.5,
            velocity_mps: -8.3,
            accel_z_gs: 0.12,
            gps_lat: -33.8688,
            gps_lon: 151.2093,
            gps_alt_m: 1230.0,
            rtk_fix: RtkFixType::RtkFixed,
            pyro_deployed: false,
            pyro_continuity: true,
            flight_state: 3,
            gps_snr: 0,
        };
        let bytes = pkt.serialize();
        assert_eq!(bytes.len(), DownlinkPacket::SIZE);
        assert_eq!(bytes[0], DOWNLINK_TYPE);
    }

    #[test]
    fn downlink_roundtrip() {
        let pkt = DownlinkPacket {
            sequence_num: 0xBEEF,
            timestamp_ms: 123_456,
            altitude_m: 3048.0,
            velocity_mps: 12.5,
            accel_z_gs: 2.1,
            gps_lat: 51.5074,
            gps_lon: -0.1278,
            gps_alt_m: 3045.0,
            rtk_fix: RtkFixType::DgpsFix,
            pyro_deployed: true,
            pyro_continuity: false,
            flight_state: 2,
            gps_snr: 0,
        };
        let bytes = pkt.serialize();
        let dec = DownlinkPacket::deserialize(&bytes).expect("deserialise failed");

        assert_eq!(dec.sequence_num, 0xBEEF);
        assert_eq!(dec.timestamp_ms, 123_456);
        assert!((dec.altitude_m - 3048.0).abs() < 1e-3);
        assert_eq!(dec.rtk_fix, RtkFixType::DgpsFix);
        assert!(dec.pyro_deployed);
        assert!(!dec.pyro_continuity);
        assert_eq!(dec.flight_state, 2);
        assert_eq!(dec.gps_snr, 0);
    }

    #[test]
    fn downlink_all_rtk_fix_types_survive_roundtrip() {
        let fixes = [
            RtkFixType::NoFix,
            RtkFixType::GpsFix,
            RtkFixType::DgpsFix,
            RtkFixType::PpsFix,
            RtkFixType::RtkFixed,
            RtkFixType::RtkFloat,
            RtkFixType::DeadReckoning,
        ];
        for fix in fixes {
            let pkt = DownlinkPacket {
                sequence_num: 0,
                timestamp_ms: 0,
                altitude_m: 0.0,
                velocity_mps: 0.0,
                accel_z_gs: 0.0,
                gps_lat: 0.0,
                gps_lon: 0.0,
                gps_alt_m: 0.0,
                rtk_fix: fix,
                pyro_deployed: false,
                pyro_continuity: false,
                flight_state: 0,
                gps_snr: 0,
            };
            let dec = DownlinkPacket::deserialize(&pkt.serialize()).unwrap();
            assert_eq!(
                dec.rtk_fix, fix,
                "fix type {:?} did not survive roundtrip",
                fix
            );
        }
    }

    #[test]
    fn downlink_deserialise_rejects_wrong_type() {
        let mut bytes = [0u8; DownlinkPacket::SIZE];
        bytes[0] = 0xFF;
        assert!(DownlinkPacket::deserialize(&bytes).is_none());
    }

    #[test]
    fn downlink_deserialise_rejects_short_slice() {
        assert!(DownlinkPacket::deserialize(&[0u8; 10]).is_none());
    }

    // ── UplinkPacket ──────────────────────────────────────────────────────────

    #[test]
    fn uplink_none_command_no_rtk() {
        let pkt = UplinkPacket {
            command: GroundCommand::None,
            rtk_data: vec![],
        };
        let bytes = pkt.serialize();
        assert_eq!(bytes.len(), 3);
        assert_eq!(bytes[0], UPLINK_TYPE);

        let dec = UplinkPacket::deserialize(&bytes).unwrap();
        assert_eq!(dec.command, GroundCommand::None);
        assert!(dec.rtk_data.is_empty());
    }

    #[test]
    fn uplink_emergency_locate_roundtrip() {
        let pkt = UplinkPacket {
            command: GroundCommand::EmergencyLocate,
            rtk_data: vec![],
        };
        let dec = UplinkPacket::deserialize(&pkt.serialize()).unwrap();
        assert_eq!(dec.command, GroundCommand::EmergencyLocate);
    }

    #[test]
    fn uplink_deploy_ejection_charge_roundtrip() {
        let pkt = UplinkPacket {
            command: GroundCommand::DeployEjectionCharge,
            rtk_data: vec![0xD3, 0x00, 0x13],
        };
        let dec = UplinkPacket::deserialize(&pkt.serialize()).unwrap();
        assert_eq!(dec.command, GroundCommand::DeployEjectionCharge);
        assert_eq!(dec.rtk_data, vec![0xD3, 0x00, 0x13]);
    }

    #[test]
    fn uplink_rtk_data_roundtrip() {
        let rtk: Vec<u8> = (0u8..200).collect();
        let pkt = UplinkPacket {
            command: GroundCommand::None,
            rtk_data: rtk.clone(),
        };
        let dec = UplinkPacket::deserialize(&pkt.serialize()).unwrap();
        assert_eq!(dec.rtk_data, rtk);
    }

    #[test]
    fn uplink_rtk_data_truncated_to_max() {
        let big = vec![0xAAu8; MAX_UPLINK_RTK + 100];
        let pkt = UplinkPacket {
            command: GroundCommand::None,
            rtk_data: big,
        };
        let bytes = pkt.serialize();
        assert!(bytes.len() <= 255, "uplink exceeds LoRa maximum payload");
        assert_eq!(bytes[2] as usize, MAX_UPLINK_RTK);
    }

    #[test]
    fn uplink_rejects_short_slice() {
        assert!(UplinkPacket::deserialize(&[]).is_none());
        assert!(UplinkPacket::deserialize(&[UPLINK_TYPE, 0]).is_none());
    }

    #[test]
    fn uplink_rejects_wrong_type() {
        let bytes = [0xFF, 0x00, 0x00];
        assert!(UplinkPacket::deserialize(&bytes).is_none());
    }

    // ── RtcmFragment ─────────────────────────────────────────────────────────

    #[test]
    fn fragment_roundtrip() {
        let frag = RtcmFragment {
            session_id: 7,
            frag_index: 2,
            total_frags: 5,
            data: vec![0xAB; 200],
        };
        let bytes = frag.serialize();
        assert_eq!(bytes[0], FRAG_TYPE);
        assert_eq!(bytes[1], 7); // session_id
        assert_eq!(bytes[2], 2); // frag_index
        assert_eq!(bytes[3], 5); // total_frags
        assert_eq!(bytes[4], 200); // data_len

        let dec = RtcmFragment::deserialize(&bytes).unwrap();
        assert_eq!(dec.session_id, 7);
        assert_eq!(dec.frag_index, 2);
        assert_eq!(dec.total_frags, 5);
        assert_eq!(dec.data.len(), 200);
        assert!(dec.data.iter().all(|&b| b == 0xAB));
    }

    #[test]
    fn fragment_data_truncated_to_max() {
        let frag = RtcmFragment {
            session_id: 0,
            frag_index: 0,
            total_frags: 1,
            data: vec![0xBBu8; MAX_FRAG_DATA + 50],
        };
        let bytes = frag.serialize();
        assert!(bytes.len() <= 255, "fragment exceeds LoRa maximum payload");
        assert_eq!(bytes[4] as usize, MAX_FRAG_DATA);
    }

    #[test]
    fn fragment_rejects_short_slice() {
        assert!(RtcmFragment::deserialize(&[]).is_none());
        assert!(RtcmFragment::deserialize(&[FRAG_TYPE; 4]).is_none());
    }

    #[test]
    fn fragment_rejects_wrong_type() {
        let bytes = [0xFF, 0, 0, 1, 0];
        assert!(RtcmFragment::deserialize(&bytes).is_none());
    }
}
