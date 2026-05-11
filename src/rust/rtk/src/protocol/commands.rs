use crate::protocol::response::ParseError;

/// Represents the commands which can be sent to the LC29H-BS device via PQTM sentences.
#[derive(Debug, Clone)]
pub enum PQTMCommand {
    CfgSvinWrite(PQTMCfgSvin),
    CfgSvinRead,

    SavePar,

    RestorePar,

    Verno,

    CfgMsgRateWrite(PQTMCfgMsgRate),
    CfgMsgRateRead(PQTMCfgMsgRateGet),

    CfgRcvrModeWrite(PQTMCfgRcvrMode),
    CfgRcvrModeRead,
}

#[derive(Debug, Clone)]
pub struct PQTMCfgSvin {
    pub mode: u8,         // 0/1/2
    pub min_dur: u32,     // seconds
    pub acc_limit_m: f32, // meters
    pub ecef_x: f64,
    pub ecef_y: f64,
    pub ecef_z: f64,
}

#[derive(Debug, Clone)]
pub struct PQTMCfgMsgRate {
    pub msg_name: PQTMMsgName,
    pub rate: u8,
    pub msg_ver: u8,
}

#[derive(Debug, Clone)]
pub enum PQTMMsgName {
    Epe,
    SvinStatus,
    RMC,
    GGA,
    GSV,
    GSA,
    VTG,
    GLL,
    ZDA,
    GRS,
    GST,
    GNS,
}

#[derive(Debug, Clone)]
pub struct PQTMCfgMsgRateGet {
    pub msg_name: String,
    pub msg_ver: String,
}

#[derive(Debug, Clone)]
pub struct PQTMCfgRcvrMode {
    pub mode: u8,
}

impl PQTMCfgMsgRateGet {
    pub fn to_fields(&self) -> String {
        format!("PQTMCFGMSGRATE,R,{},{}", self.msg_name, self.msg_ver,)
    }
}

impl PQTMCfgMsgRate {
    pub fn to_fields(&self) -> String {
        // Standard NMEA msg types should not have msg_ver:
        let msg_ver_needed = matches!(self.msg_name, PQTMMsgName::Epe | PQTMMsgName::SvinStatus);
        
        if msg_ver_needed {
            format!(
                "PQTMCFGMSGRATE,W,{},{},{}",
                self.msg_name.clone().as_str(),
                self.rate,
                self.msg_ver,
            )
        } else {
            format!(
                "PQTMCFGMSGRATE,W,{},{}",
                self.msg_name.clone().as_str(),
                self.rate,
            )
        }
    }

    pub fn from_fields<'a, I>(it: &mut I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = &'a str>,
    {
        let msg_name_str = it
            .next()
            .ok_or(ParseError::ParsingError("msg_name not found"))?;
        let msg_name =
            PQTMMsgName::parse(msg_name_str).ok_or(ParseError::ParsingError("invalid msg_name"))?;

        let rate_str = it
            .next()
            .ok_or(ParseError::ParsingError("rate not found"))?;
        let rate: u8 = rate_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid rate"))?;

        let msg_ver_str = it
            .next()
            .ok_or(ParseError::ParsingError("msg_ver not found"))?;
        let msg_ver: u8 = msg_ver_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid msg_ver"))?;

        Ok(PQTMCfgMsgRate {
            msg_name,
            rate,
            msg_ver,
        })
    }
}

impl PQTMCfgSvin {
    pub fn to_fields(&self) -> String {
        format!(
            "PQTMCFGSVIN,W,{},{},{:.1},{:.4},{:.4},{:.4}",
            self.mode, self.min_dur, self.acc_limit_m, self.ecef_x, self.ecef_y, self.ecef_z,
        )
    }

    pub fn from_fields<'a, I>(it: &mut I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = &'a str>,
    {
        let mode_str = it
            .next()
            .ok_or(ParseError::ParsingError("mode not found"))?;
        let mode: u8 = mode_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid mode"))?;

        let min_dur_str = it
            .next()
            .ok_or(ParseError::ParsingError("min_dur not found"))?;
        let min_dur: u32 = min_dur_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid min_dur"))?;

        let acc_limit_str = it
            .next()
            .ok_or(ParseError::ParsingError("acc_limit_m not found"))?;
        let acc_limit_m: f32 = acc_limit_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid acc_limit_m"))?;

        let ecef_x_str = it
            .next()
            .ok_or(ParseError::ParsingError("ecef_x not found"))?;
        let ecef_x: f64 = ecef_x_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid ecef_x"))?;

        let ecef_y_str = it
            .next()
            .ok_or(ParseError::ParsingError("ecef_y not found"))?;
        let ecef_y: f64 = ecef_y_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid ecef_y"))?;

        let ecef_z_str = it
            .next()
            .ok_or(ParseError::ParsingError("ecef_z not found"))?;
        let ecef_z: f64 = ecef_z_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid ecef_z"))?;

        Ok(PQTMCfgSvin {
            mode,
            min_dur,
            acc_limit_m,
            ecef_x,
            ecef_y,
            ecef_z,
        })
    }
}

impl PQTMMsgName {
    pub fn as_str(self) -> &'static str {
        match self {
            PQTMMsgName::SvinStatus => "PQTMSVINSTATUS",
            PQTMMsgName::Epe => "PQTMEPE",
            PQTMMsgName::RMC => "RMC",
            PQTMMsgName::GGA => "GGA",
            PQTMMsgName::GSV => "GSV",
            PQTMMsgName::GSA => "GSA",
            PQTMMsgName::VTG => "VTG",
            PQTMMsgName::GLL => "GLL",
            PQTMMsgName::ZDA => "ZDA",
            PQTMMsgName::GRS => "GRS",
            PQTMMsgName::GST => "GST",
            PQTMMsgName::GNS => "GNS",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "PQTMSVINSTATUS" => Some(PQTMMsgName::SvinStatus),
            "PQTMEPE" => Some(PQTMMsgName::Epe),
            "RMC" => Some(PQTMMsgName::RMC),
            "GGA" => Some(PQTMMsgName::GGA),
            "GSV" => Some(PQTMMsgName::GSV),
            "GSA" => Some(PQTMMsgName::GSA),
            "VTG" => Some(PQTMMsgName::VTG),
            "GLL" => Some(PQTMMsgName::GLL),
            "ZDA" => Some(PQTMMsgName::ZDA),
            "GRS" => Some(PQTMMsgName::GRS),
            "GST" => Some(PQTMMsgName::GST),
            "GNS" => Some(PQTMMsgName::GNS),
            _ => None,
        }
    }
}

impl PQTMCfgRcvrMode {
    pub fn to_fields(&self) -> String {
        format!("PQTMCFGRCVRMODE,W,{}", self.mode)
    }

    pub fn from_fields<'a, I>(it: &mut I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = &'a str>,
    {
        let mode_str = it
            .next()
            .ok_or(ParseError::ParsingError("mode not found"))?;
        let mode: u8 = mode_str
            .parse()
            .map_err(|_| ParseError::ParsingError("invalid mode"))?;

        Ok(PQTMCfgRcvrMode { mode })
    }
}
