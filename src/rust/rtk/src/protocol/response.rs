use crate::protocol::nmea::{GgaData, GsvData};
use crate::protocol::pair::PairResponse;

use super::commands::{PQTMCfgMsgRate, PQTMCfgSvin};

#[derive(Debug, Clone)]
pub enum WireMessage {
    PQTMMessage(PQTMResponse),
    PairMessage(PairResponse),
    NmeaGga(GgaData),
    NmeaGsv(GsvData),
}

/// Represents the output from the LC29H-BS device.
#[derive(Debug, Clone)]
pub enum PQTMResponse {
    CfgSvinWriteOk,
    CfgSvinReadOk(PQTMCfgSvin),
    CfgSvinError(PQTMModuleError),

    SaveParOk,
    SaveParError(PQTMModuleError),

    RestoreParOk,
    RestoreParError(PQTMModuleError),

    Verno(PQTMVerNo),
    VernoError(PQTMModuleError),

    CfgMsgRateWriteOk,
    CfgMsgRateReadOk(PQTMCfgMsgRate),
    CfgMsgRateError(PQTMModuleError),

    Epe(PQTMEpe),
    SvinStatus(PQTMSvinStatus),
}

/// Represents errors returned by the GPS module.
#[derive(Debug, Clone)]
pub enum PQTMModuleError {
    InvalidParameters,
    ExecutionFailed,
    Unknown(u8),
}

/// Represents errors that can occur when parsing a PQTM sentence.
#[derive(Debug, Clone)]
pub enum ParseError {
    StartDelimiterNotFound,
    NoSentence,
    NoStatusField,
    InvalidStatusField,
    ChecksumNotFound,
    ChecksumLengthInvalid,
    ChecksumMismatch,
    NoErrorCode,
    ParsingError(&'static str),
}

/// Represents errors that can occur when processing a PQTM response.
#[derive(Debug, Clone)]
pub enum ResponseError {
    ModuleError(PQTMModuleError),
    ParseError(ParseError),
}

#[derive(Debug, Clone)]
pub struct PQTMSvinStatus {
    _msg_ver: String,
    pub time_of_week: u64, // ms
    pub valid: u8,         // 0 - invalid, 1 - in-progress, 2 - valid
    _reserved1: String,
    _reserved2: String,
    pub observations: u32,
    pub config_duration: u32,
    pub mean_x: f64,   // mean position in ECEF (m)
    pub mean_y: f64,   // mean position in ECEF (m)
    pub mean_z: f64,   // mean position in ECEF (m)
    pub mean_acc: f32, // mean accuracy (m)
}

#[derive(Debug, Clone)]
pub struct PQTMEpe {
    _msg_ver: String,
    pub epe_north: f32, // North position error (m)
    pub epe_east: f32,  // East position error (m)
    pub epe_down: f32,  // Down position error (m)
    pub epe_2d: f32,    // 2D position error (m)
    pub epe_3d: f32,    // 3D position error (m)
}

#[derive(Debug, Clone)]
pub struct PQTMVerNo {
    pub version: String,
    pub build_date: String,
    pub build_time: String,
}

impl PQTMVerNo {
    pub fn from_fields<'a, I>(it: &mut I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = &'a str>,
    {
        let version = it
            .next()
            .ok_or(ParseError::ParsingError("version not found"))?
            .to_string();
        let build_date = it
            .next()
            .ok_or(ParseError::ParsingError("build_date not found"))?
            .to_string();
        let build_time = it
            .next()
            .ok_or(ParseError::ParsingError("build_time not found"))?
            .to_string();
        Ok(PQTMVerNo {
            version,
            build_date,
            build_time,
        })
    }
}

impl PQTMEpe {
    pub fn from_fields<'a, I>(it: &mut I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = &'a str>,
    {
        let _msg_ver = it
            .next()
            .ok_or(ParseError::ParsingError("msg_ver not found"))?
            .to_string();

        let epe_north_str = it
            .next()
            .ok_or(ParseError::ParsingError("epe_north not found"))?;
        let epe_north: f32 = epe_north_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid epe_north"))?;

        let epe_east_str = it
            .next()
            .ok_or(ParseError::ParsingError("epe_east not found"))?;
        let epe_east: f32 = epe_east_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid epe_east"))?;

        let epe_down_str = it
            .next()
            .ok_or(ParseError::ParsingError("epe_down not found"))?;
        let epe_down: f32 = epe_down_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid epe_down"))?;

        let epe_2d_str = it
            .next()
            .ok_or(ParseError::ParsingError("epe_2d not found"))?;
        let epe_2d: f32 = epe_2d_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid epe_2d"))?;

        let epe_3d_str = it
            .next()
            .ok_or(ParseError::ParsingError("epe_3d not found"))?;
        let epe_3d: f32 = epe_3d_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid epe_3d"))?;

        Ok(PQTMEpe {
            _msg_ver,
            epe_north,
            epe_east,
            epe_down,
            epe_2d,
            epe_3d,
        })
    }
}

impl PQTMSvinStatus {
    pub fn from_fields<'a, I>(it: &mut I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = &'a str>,
    {
        let _msg_ver = it
            .next()
            .ok_or(ParseError::ParsingError("msg_ver not found"))?
            .to_string();

        let time_of_week_str = it
            .next()
            .ok_or(ParseError::ParsingError("time_of_week not found"))?;
        let time_of_week: u64 = time_of_week_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid time_of_week"))?;

        let valid_str = it
            .next()
            .ok_or(ParseError::ParsingError("valid not found"))?;
        let valid: u8 = valid_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid valid"))?;

        let _reserved1 = it
            .next()
            .ok_or(ParseError::ParsingError("reserved1 not found"))?
            .to_string();

        let _reserved2 = it
            .next()
            .ok_or(ParseError::ParsingError("reserved2 not found"))?
            .to_string();

        let observations_str = it
            .next()
            .ok_or(ParseError::ParsingError("observations not found"))?;
        let observations: u32 = observations_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid observations"))?;

        let config_duration_str = it
            .next()
            .ok_or(ParseError::ParsingError("config_duration not found"))?;
        let config_duration: u32 = config_duration_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid config_duration"))?;

        let mean_x_str = it
            .next()
            .ok_or(ParseError::ParsingError("mean_x not found"))?;
        let mean_x: f64 = mean_x_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid mean_x"))?;

        let mean_y_str = it
            .next()
            .ok_or(ParseError::ParsingError("mean_y not found"))?;
        let mean_y: f64 = mean_y_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid mean_y"))?;

        let mean_z_str = it
            .next()
            .ok_or(ParseError::ParsingError("mean_z not found"))?;
        let mean_z: f64 = mean_z_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid mean_z"))?;

        let mean_acc_str = it
            .next()
            .ok_or(ParseError::ParsingError("mean_acc not found"))?;
        let mean_acc: f32 = mean_acc_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid mean_acc"))?;
        Ok(PQTMSvinStatus {
            _msg_ver,
            time_of_week,
            valid,
            _reserved1,
            _reserved2,
            observations,
            config_duration,
            mean_x,
            mean_y,
            mean_z,
            mean_acc,
        })
    }
}

impl From<u8> for PQTMModuleError {
    fn from(code: u8) -> Self {
        match code {
            1 => PQTMModuleError::InvalidParameters,
            2 => PQTMModuleError::ExecutionFailed,
            _ => PQTMModuleError::Unknown(code),
        }
    }
}
