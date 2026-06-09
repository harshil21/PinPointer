use crate::protocol::response::ParseError;

#[derive(Debug, Clone)]
pub enum PairCommand {
    RtcmSetOutputMode(PairRTCMSetOutputMode),     // PAIR432
    RtcmGetOutputMode,                            // PAIR433
    RtcmSetOutputAntPnt(PairRTCMSetOutputAntPnt), // PAIR434
    RtcmGetOutputAntPnt,                          // PAIR435
    RtcmSetOutputEphemeris(PairRTCMSetOutputEphemeris), // PAIR436
    RtcmGetOutputEphemeris,                       // PAIR437
    NvramSaveSetting,                             // PAIR513
    CommonSetNmeaOutputRate(PairCommonSetNmeaOutputRate), // PAIR062
}

#[derive(Debug, Clone)]
pub enum PairResponse {
    ACK(PairACK),                                    // PAIR001
    RtcmOutputMode(PairRTCMSetOutputMode),           // PAIR433 response
    RtcmOutputAntPnt(PairRTCMSetOutputAntPnt),       // PAIR435 response
    RtcmOutputEphemeris(PairRTCMSetOutputEphemeris), // PAIR437 response
    RequestAiding(PairRequestAiding),                // PAIR010
    SystemWakeUp,                                    // PAIR012
}

#[derive(Debug, Clone)]
pub struct PairACK {
    pub command_id: u16,
    pub result: AckResult,
}

impl PairACK {
    pub fn from_fields<'a, I>(it: &mut I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = &'a str>,
    {
        let command_id = it
            .next()
            .ok_or(ParseError::ParsingError("command_id not found"))?;
        let command_id: u16 = command_id
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid command_id"))?;
        let result_str = it
            .next()
            .ok_or(ParseError::ParsingError("result not found"))?;
        let result_u8: u8 = result_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid result"))?;
        let result = match result_u8 {
            0 => AckResult::Success,
            1 => AckResult::Processing,
            2 => AckResult::Failed,
            3 => AckResult::NotSupported,
            4 => AckResult::Error,
            5 => AckResult::Busy,
            _ => return Err(ParseError::ParsingError("invalid result value")),
        };
        Ok(PairACK { command_id, result })
    }

    pub fn to_fields(&self) -> String {
        format!("PAIR001,{},{}", self.command_id, self.result.clone() as u8)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AckResult {
    Success = 0,
    Processing = 1,
    Failed = 2,
    NotSupported = 3,
    Error = 4,
    Busy = 5,
}

#[derive(Debug, Clone)]
pub struct PairRTCMSetOutputMode {
    pub mode: RtcmMode,
}

impl PairRTCMSetOutputMode {
    pub fn from_fields<'a, I>(it: &mut I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = &'a str>,
    {
        let mode_str = it
            .next()
            .ok_or(ParseError::ParsingError("mode not found"))?;
        let mode_i8: i8 = mode_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid mode"))?;
        let mode = match mode_i8 {
            -1 => RtcmMode::Disable,
            0 => RtcmMode::Rtcm3Msm4,
            1 => RtcmMode::Rtcm3Msm7,
            _ => return Err(ParseError::ParsingError("invalid mode value")),
        };
        Ok(PairRTCMSetOutputMode { mode })
    }

    pub fn to_fields(&self) -> String {
        format!("PAIR432,{}", self.mode.clone() as i8)
    }
}

#[derive(Debug, Clone)]
pub enum RtcmMode {
    Disable = -1,
    Rtcm3Msm4 = 0,
    Rtcm3Msm7 = 1,
}

pub type PairRTCMGetOutputMode = PairRTCMSetOutputMode;

/// Enable/disable outputting stationary RTK reference station ARP (message type 1005).
#[derive(Debug, Clone)]
pub struct PairRTCMSetOutputAntPnt {
    pub ant_pnt: RtcmAntPnt,
}

impl PairRTCMSetOutputAntPnt {
    pub fn from_fields<'a, I>(it: &mut I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = &'a str>,
    {
        let ant_pnt_str = it
            .next()
            .ok_or(ParseError::ParsingError("ant_pnt not found"))?;
        let ant_pnt_u8: u8 = ant_pnt_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid ant_pnt"))?;
        let ant_pnt = match ant_pnt_u8 {
            0 => RtcmAntPnt::Disable,
            1 => RtcmAntPnt::Enable,
            _ => return Err(ParseError::ParsingError("invalid ant_pnt value")),
        };
        Ok(PairRTCMSetOutputAntPnt { ant_pnt })
    }

    pub fn to_fields(&self) -> String {
        format!("PAIR434,{}", self.ant_pnt.clone() as u8)
    }
}

pub type PairRTCMGetOutputAntPnt = PairRTCMSetOutputAntPnt;

#[derive(Debug, Clone)]
pub enum RtcmAntPnt {
    Disable = 0,
    Enable = 1,
}

#[derive(Debug, Clone)]
pub struct PairRTCMSetOutputEphemeris {
    pub ephemeris: RtcmEphemeris,
}

impl PairRTCMSetOutputEphemeris {
    pub fn from_fields<'a, I>(it: &mut I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = &'a str>,
    {
        let ephemeris_str = it
            .next()
            .ok_or(ParseError::ParsingError("ephemeris not found"))?;
        let ephemeris_u8: u8 = ephemeris_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid ephemeris"))?;
        let ephemeris = match ephemeris_u8 {
            0 => RtcmEphemeris::Disable,
            1 => RtcmEphemeris::Enable,
            _ => return Err(ParseError::ParsingError("invalid ephemeris value")),
        };
        Ok(PairRTCMSetOutputEphemeris { ephemeris })
    }

    pub fn to_fields(&self) -> String {
        format!("PAIR436,{}", self.ephemeris.clone() as u8)
    }
}

pub type PairRTCMGetOutputEphemeris = PairRTCMSetOutputEphemeris;

#[derive(Debug, Clone)]
pub enum RtcmEphemeris {
    Disable = 0,
    Enable = 1,
}

#[derive(Debug, Clone)]
pub struct PairRequestAiding {
    /// Type of data to be updated
    pub aiding_type: AidingType,
    /// Type of required GNSS data
    pub gnss_system: GnssSystem,
    /// Week number (accommodating rollover)
    pub week_number: u16,
    /// Time of week in seconds
    pub time_of_week: u64,
}

impl PairRequestAiding {
    pub fn from_fields<'a, I>(it: &mut I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = &'a str>,
    {
        let aiding_type_str = it
            .next()
            .ok_or(ParseError::ParsingError("aiding_type not found"))?;
        let aiding_type_u8: u8 = aiding_type_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid aiding_type"))?;
        let aiding_type = match aiding_type_u8 {
            0 => AidingType::EpoData,
            1 => AidingType::Time,
            2 => AidingType::Location,
            _ => return Err(ParseError::ParsingError("invalid aiding_type value")),
        };

        let gnss_system_str = it
            .next()
            .ok_or(ParseError::ParsingError("gnss_system not found"))?;
        let gnss_system_u8: u8 = gnss_system_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid gnss_system"))?;
        let gnss_system = match gnss_system_u8 {
            0 => GnssSystem::Gps,
            1 => GnssSystem::Glonass,
            2 => GnssSystem::Galileo,
            3 => GnssSystem::BeiDou,
            4 => GnssSystem::Qzss,
            _ => return Err(ParseError::ParsingError("invalid gnss_system value")),
        };

        let week_number_str = it
            .next()
            .ok_or(ParseError::ParsingError("week_number not found"))?;
        let week_number: u16 = week_number_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid week_number"))?;

        let time_of_week_str = it
            .next()
            .ok_or(ParseError::ParsingError("time_of_week not found"))?;
        let time_of_week: u64 = time_of_week_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid time_of_week"))?;

        Ok(PairRequestAiding {
            aiding_type,
            gnss_system,
            week_number,
            time_of_week,
        })
    }

    pub fn to_fields(&self) -> String {
        format!(
            "PAIR010,{},{},{},{}",
            self.aiding_type.clone() as u8,
            self.gnss_system.clone() as u8,
            self.week_number,
            self.time_of_week,
        )
    }
}

#[derive(Debug, Clone)]
pub enum AidingType {
    EpoData = 0,
    Time = 1,
    Location = 2,
}

#[derive(Debug, Clone)]
pub enum GnssSystem {
    Gps = 0,
    Glonass = 1,
    Galileo = 2,
    BeiDou = 3,
    Qzss = 4,
}

#[derive(Debug, Clone)]
pub struct PairCommonSetNmeaOutputRate {
    pub _type: NmeaOutputRateTypes,
    pub output_rate: u8,
}

impl PairCommonSetNmeaOutputRate {
    pub fn from_fields<'a, I>(it: &mut I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = &'a str>,
    {
        let nmea_type = it
            .next()
            .ok_or(ParseError::ParsingError("type not found"))?;
        let nmea_type_u8: u8 = nmea_type
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid type"))?;
        let nmea_type_enum = match nmea_type_u8 {
            0 => NmeaOutputRateTypes::GGA,
            1 => NmeaOutputRateTypes::GLL,
            2 => NmeaOutputRateTypes::GSA,
            3 => NmeaOutputRateTypes::GSV,
            4 => NmeaOutputRateTypes::RMC,
            5 => NmeaOutputRateTypes::VTG,
            6 => NmeaOutputRateTypes::ZDA,
            7 => NmeaOutputRateTypes::GRS,
            8 => NmeaOutputRateTypes::GST,
            9 => NmeaOutputRateTypes::GNS,
            _ => return Err(ParseError::ParsingError("invalid type value")),
        };

        let output_rate = it
            .next()
            .ok_or(ParseError::ParsingError("output_rate not found"))?;
        let output_rate: u8 = output_rate
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid output_rate"))?;

        Ok(PairCommonSetNmeaOutputRate {
            _type: nmea_type_enum,
            output_rate: output_rate,
        })
    }

    pub fn to_fields(&self) -> String {
        format!(
            "PAIR062,{},{}",
            self._type.clone() as u8,
            self.output_rate.clone() as u8,
        )
    }
}
#[derive(Debug, Clone)]
pub enum NmeaOutputRateTypes {
    GGA = 0,
    GLL = 1,
    GSA = 2,
    GSV = 3,
    RMC = 4,
    VTG = 5,
    ZDA = 6,
    GRS = 7,
    GST = 8,
    GNS = 9,
}
