use crate::protocol::commands::{PQTMCfgMsgRate, PQTMCfgNmeaDp, PQTMCfgSvin};
use crate::protocol::helpers::{StatusField, parse_status_and_rest, wrap_sentence};
use crate::protocol::pair::{
    PairACK, PairCommand, PairCommonSetNmeaOutputRate, PairRTCMSetOutputAntPnt,
    PairRTCMSetOutputEphemeris, PairRTCMSetOutputMode, PairRequestAiding, PairResponse,
};
use crate::protocol::response::{
    PQTMEpe, PQTMModuleError, PQTMResponse, PQTMSvinStatus, PQTMVerNo, ParseError,
};

use super::commands::PQTMCommand;
use super::helpers::unwrap_sentence;

pub trait Serialize {
    fn to_sentence(&self) -> String;
}

pub trait Deserialize: Sized {
    type Error;
    fn from_sentence(s: &str) -> Result<Self, Self::Error>;
}

impl Serialize for PQTMCommand {
    fn to_sentence(&self) -> String {
        match self {
            PQTMCommand::CfgSvinWrite(cfg) => wrap_sentence(&cfg.to_fields()),
            PQTMCommand::CfgSvinRead => wrap_sentence("PQTMCFGSVIN,R"),
            PQTMCommand::SavePar => wrap_sentence("PQTMSAVEPAR"),
            PQTMCommand::RestorePar => wrap_sentence("PQTMRESTOREPAR"),
            PQTMCommand::Verno => wrap_sentence("PQTMVERNO"),
            PQTMCommand::CfgMsgRateWrite(cfg) => wrap_sentence(&cfg.to_fields()),
            PQTMCommand::CfgMsgRateRead(cfg_get) => wrap_sentence(&cfg_get.to_fields()),
            PQTMCommand::CfgRcvrModeRead => wrap_sentence("PQTMCFGRCVRMODE"),
            PQTMCommand::CfgRcvrModeWrite(cfg) => wrap_sentence(&cfg.to_fields()),
            PQTMCommand::CfgNmeaDpWrite(cfg) => wrap_sentence(&cfg.to_fields()),
            PQTMCommand::CfgNmeaDpRead => wrap_sentence("PQTMCFGNMEADP,R")
        }
    }
}

impl Deserialize for PQTMResponse {
    type Error = ParseError;

    fn from_sentence(s: &str) -> Result<PQTMResponse, Self::Error> {
        let payload = unwrap_sentence(s)?;
        let mut parts = payload.split(",");
        let header = parts.next().ok_or(ParseError::NoSentence)?;

        match header {
            "PQTMSAVEPAR" => match parse_status_and_rest(parts)? {
                StatusField::Ok(_) => Ok(PQTMResponse::SaveParOk),
                StatusField::Err(e) => Ok(PQTMResponse::SaveParError(e)),
            },
            "PQTMRESTOREPAR" => match parse_status_and_rest(parts)? {
                StatusField::Ok(_) => Ok(PQTMResponse::RestoreParOk),
                StatusField::Err(e) => Ok(PQTMResponse::RestoreParError(e)),
            },
            "PQTMVERNO" => {
                // This does unfortunately not follow the OK/ERROR pattern
                // Check if first field is "ERROR"
                let first = parts.clone().next().ok_or(ParseError::NoSentence)?;
                if first == "ERROR" {
                    parts.next(); // skip "ERROR"
                    let code: u8 = parts
                        .next()
                        .ok_or(ParseError::NoErrorCode)?
                        .parse()
                        .map_err(|_| ParseError::NoErrorCode)?;
                    Ok(PQTMResponse::VernoError(PQTMModuleError::from(code)))
                } else {
                    // Direct data: VerStr,BuildDate,BuildTime
                    let verno = PQTMVerNo::from_fields(&mut parts)?;
                    Ok(PQTMResponse::Verno(verno))
                }
            }
            "PQTMCFGSVIN" => {
                match parse_status_and_rest(parts)? {
                    StatusField::Ok(mut rest) => {
                        if rest.clone().next().is_none() {
                            // Write response: OK only:
                            Ok(PQTMResponse::CfgSvinWriteOk)
                        } else {
                            // Read response:
                            Ok(PQTMResponse::CfgSvinReadOk(PQTMCfgSvin::from_fields(
                                &mut rest,
                            )?))
                        }
                    }
                    StatusField::Err(e) => Ok(PQTMResponse::CfgSvinError(e)),
                }
            }
            "PQTMCFGMSGRATE" => {
                match parse_status_and_rest(parts)? {
                    StatusField::Ok(mut rest) => {
                        if rest.clone().next().is_none() {
                            // Write response: OK only:
                            Ok(PQTMResponse::CfgMsgRateWriteOk)
                        } else {
                            // Read response:
                            Ok(PQTMResponse::CfgMsgRateReadOk(PQTMCfgMsgRate::from_fields(
                                &mut rest,
                            )?))
                        }
                    }
                    StatusField::Err(e) => Ok(PQTMResponse::CfgMsgRateError(e)),
                }
            }
            "PQTMEPE" => Ok(PQTMResponse::Epe(PQTMEpe::from_fields(&mut parts)?)),
            "PQTMSVINSTATUS" => Ok(PQTMResponse::SvinStatus(PQTMSvinStatus::from_fields(
                &mut parts,
            )?)),
            "PQTMCFGRCVRMODE" => match parse_status_and_rest(parts)? {
                StatusField::Ok(_) => Ok(PQTMResponse::CfgRcvrModeWriteOk),
                StatusField::Err(e) => Ok(PQTMResponse::CfgRcvrError(e)),
            },
            "PQTMCFGNMEADP" => {
                match parse_status_and_rest(parts)? {
                    StatusField::Ok(mut rest) => {
                        if rest.clone().next().is_none() {
                            // Write response: OK only:
                            Ok(PQTMResponse::CfgNmeaDpWriteOk)
                        } else {
                            // Read response:
                            Ok(PQTMResponse::CfgNmeaDpReadOk(PQTMCfgNmeaDp::from_fields(
                                &mut rest,
                            )?))
                        }
                    }
                    StatusField::Err(e) => Ok(PQTMResponse::CfgNmeaDpError(e)),
                }
            },
            _ => Err(ParseError::ParsingError("Unknown sentence header")),
        }
    }
}

impl Serialize for PairCommand {
    fn to_sentence(&self) -> String {
        match self {
            PairCommand::RtcmSetOutputMode(cfg) => wrap_sentence(&cfg.to_fields()),
            PairCommand::RtcmGetOutputMode => wrap_sentence("PAIR433"),
            PairCommand::RtcmSetOutputAntPnt(cfg) => wrap_sentence(&cfg.to_fields()),
            PairCommand::RtcmGetOutputAntPnt => wrap_sentence("PAIR435"),
            PairCommand::RtcmSetOutputEphemeris(cfg) => wrap_sentence(&cfg.to_fields()),
            PairCommand::RtcmGetOutputEphemeris => wrap_sentence("PAIR437"),
            PairCommand::NvramSaveSetting => wrap_sentence("PAIR513"),
            PairCommand::CommonSetNmeaOutputRate(cfg) => wrap_sentence(&cfg.to_fields()),
        }
    }
}

impl Deserialize for PairResponse {
    type Error = ParseError;

    fn from_sentence(s: &str) -> Result<PairResponse, Self::Error> {
        let payload = unwrap_sentence(s)?;
        let mut parts = payload.split(',');
        let header = parts.next().ok_or(ParseError::NoSentence)?;

        match header {
            "PAIR001" => {
                let ack = PairACK::from_fields(&mut parts)?;
                Ok(PairResponse::ACK(ack))
            }
            "PAIR433" => {
                let mode = PairRTCMSetOutputMode::from_fields(&mut parts)?;
                Ok(PairResponse::RtcmOutputMode(mode))
            }
            "PAIR435" => {
                let antpnt = PairRTCMSetOutputAntPnt::from_fields(&mut parts)?;
                Ok(PairResponse::RtcmOutputAntPnt(antpnt))
            }
            "PAIR437" => {
                let ephemeris = PairRTCMSetOutputEphemeris::from_fields(&mut parts)?;
                Ok(PairResponse::RtcmOutputEphemeris(ephemeris))
            }
            "PAIR012" => Ok(PairResponse::SystemWakeUp),
            "PAIR010" => {
                let aiding = PairRequestAiding::from_fields(&mut parts)?;
                Ok(PairResponse::RequestAiding(aiding))
            }
            _ => Err(ParseError::ParsingError("Unknown PAIR sentence header")),
        }
    }
}
