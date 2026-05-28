use std::time::Duration;

use crate::protocol::commands::{
    PQTMCfgMsgRate, PQTMCfgMsgRateGet, PQTMCfgRcvrMode, PQTMCfgSvin, PQTMCommand, PQTMCfgNmeaDp
};
use crate::protocol::pair::{
    AckResult, PairACK, PairCommand, PairCommonSetNmeaOutputRate, PairRTCMSetOutputAntPnt,
    PairRTCMSetOutputEphemeris, PairRTCMSetOutputMode, PairResponse, RtcmAntPnt, RtcmEphemeris,
    RtcmMode,
};
use crate::protocol::response::{PQTMResponse, PQTMVerNo, ParseError, ResponseError};

use crate::port::BaseGPS;

macro_rules! command_methods {
    (
        $(
            $method_name:ident (
                $($arg_name:ident: $arg_type:ty),*
            ) -> $return_type:ty {
                command: $command_variant:expr,
                ok: $ok_pattern:pat => $ok_expr:expr,
                err: $err_pattern:pat => $err_expr:expr,
            }
        )*
    ) => {
        $(
            pub fn $method_name(
                &mut self,
                $($arg_name: $arg_type,)*
                timeout: Duration,
            ) -> Result<$return_type, ResponseError> {
                let resp = self.send_command($command_variant, timeout)?;
                match resp {
                    $ok_pattern => Ok($ok_expr),
                    $err_pattern => Err(ResponseError::ModuleError($err_expr)),
                    _ => Err(ResponseError::ParseError(
                        ParseError::ParsingError(concat!(
                            "unexpected response to ",
                            stringify!($method_name)
                        ))
                    )),
                }
            }
        )*
    };
}

macro_rules! pair_get_methods {
    (
        $(
            $method_name:ident (
                $($arg_name:ident: $arg_type:ty),*
            ) -> $return_type:ty {
                command: $command_variant:expr,
                response: $response_pattern:pat => $response_expr:expr,
            }
        )*
    ) => {
        $(
            pub fn $method_name(
                &mut self,
                $($arg_name: $arg_type,)*
                timeout: Duration,
            ) -> Result<$return_type, ResponseError> {
                let (ack, resp) = self.send_pair_get($command_variant, timeout)?;

                // Validate ACK is success (already checked in send_pair_get, but be explicit)
                if ack.result != AckResult::Success {
                    return Err(ResponseError::ParseError(
                        ParseError::ParsingError("PAIR command ACK failed")
                    ));
                }

                match resp {
                    $response_pattern => Ok($response_expr),
                    _ => Err(ResponseError::ParseError(
                        ParseError::ParsingError(concat!(
                            "unexpected response to ",
                            stringify!($method_name)
                        ))
                    )),
                }
            }
        )*
    };
}

macro_rules! pair_set_methods {
    (
        $(
            $method_name:ident (
                $($arg_name:ident: $arg_type:ty),*
            ) {
                command: $command_variant:expr,
            }
        )*
    ) => {
        $(
            pub fn $method_name(
                &mut self,
                $($arg_name: $arg_type,)*
                timeout: Duration,
            ) -> Result<PairACK, ResponseError> {
                self.send_pair_set($command_variant, timeout)
            }
        )*
    };
}

impl BaseGPS {
    command_methods! {
        verno() -> PQTMVerNo {
            command: PQTMCommand::Verno,
            ok: PQTMResponse::Verno(info) => info,
            err: PQTMResponse::VernoError(e) => e,
        }

        save_par() -> () {
            command: PQTMCommand::SavePar,
            ok: PQTMResponse::SaveParOk => (),
            err: PQTMResponse::SaveParError(e) => e,
        }

        restore_par() -> () {
            command: PQTMCommand::RestorePar,
            ok: PQTMResponse::RestoreParOk => (),
            err: PQTMResponse::RestoreParError(e) => e,
        }

        cfg_svin_read() -> PQTMCfgSvin {
            command: PQTMCommand::CfgSvinRead,
            ok: PQTMResponse::CfgSvinReadOk(cfg) => cfg,
            err: PQTMResponse::CfgSvinError(e) => e,
        }

        cfg_svin_write(cfg: PQTMCfgSvin) -> () {
            command: PQTMCommand::CfgSvinWrite(cfg),
            ok: PQTMResponse::CfgSvinWriteOk => (),
            err: PQTMResponse::CfgSvinError(e) => e,
        }

        cfg_msgrate_write(rate: PQTMCfgMsgRate) -> () {
            command: PQTMCommand::CfgMsgRateWrite(rate),
            ok: PQTMResponse::CfgMsgRateWriteOk => (),
            err: PQTMResponse::CfgMsgRateError(e) => e,
        }

        cfg_msgrate_read(req: PQTMCfgMsgRateGet) -> PQTMCfgMsgRate {
            command: PQTMCommand::CfgMsgRateRead(req),
            ok: PQTMResponse::CfgMsgRateReadOk(rate) => rate,
            err: PQTMResponse::CfgMsgRateError(e) => e,
        }

        cfg_rcvrmode_write(mode: PQTMCfgRcvrMode) -> () {
            command: PQTMCommand::CfgRcvrModeWrite(mode),
            ok: PQTMResponse::CfgRcvrModeWriteOk => (),
            err: PQTMResponse::CfgRcvrError(e) => e,
        }

        cfg_nmea_dp_read() -> PQTMCfgNmeaDp {
            command: PQTMCommand::CfgNmeaDpRead,
            ok: PQTMResponse::CfgNmeaDpReadOk(nmea_dp) => nmea_dp,
            err: PQTMResponse::CfgNmeaDpError(e) => e,
        }

        cfg_nmea_dp_write(cfg: PQTMCfgNmeaDp) -> () {
            command: PQTMCommand::CfgNmeaDpWrite(cfg),
            ok: PQTMResponse::CfgNmeaDpWriteOk => (),
            err: PQTMResponse::CfgNmeaDpError(e) => e,
        }
    }
    // PAIR GET commands (wait for ACK + response)
    pair_get_methods! {
        pair_get_rtcm_mode() -> RtcmMode {
            command: PairCommand::RtcmGetOutputMode,
            response: PairResponse::RtcmOutputMode(mode) => mode.mode,
        }

        pair_get_rtcm_antpnt() -> RtcmAntPnt {
            command: PairCommand::RtcmGetOutputAntPnt,
            response: PairResponse::RtcmOutputAntPnt(antpnt) => antpnt.ant_pnt,
        }

        pair_get_rtcm_ephemeris() -> RtcmEphemeris {
            command: PairCommand::RtcmGetOutputEphemeris,
            response: PairResponse::RtcmOutputEphemeris(eph) => eph.ephemeris,
        }
    }

    // PAIR SET commands (only wait for ACK)
    pair_set_methods! {
        pair_set_rtcm_mode(mode: PairRTCMSetOutputMode) {
            command: PairCommand::RtcmSetOutputMode(mode),
        }

        pair_set_rtcm_antpnt(antpnt: PairRTCMSetOutputAntPnt) {
            command: PairCommand::RtcmSetOutputAntPnt(antpnt),
        }

        pair_set_rtcm_ephemeris(ephemeris: PairRTCMSetOutputEphemeris) {
            command: PairCommand::RtcmSetOutputEphemeris(ephemeris),
        }

        pair_nvram_save_setting() {
            command: PairCommand::NvramSaveSetting,
        }

        pair_common_set_nmea_output_rate(rate: PairCommonSetNmeaOutputRate) {
            command: PairCommand::CommonSetNmeaOutputRate(rate),
        }
    }
}
