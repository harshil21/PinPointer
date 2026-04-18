use crate::protocol::response::{PQTMModuleError, ParseError};

#[derive(Debug)]
pub enum StatusField<'a> {
    Ok(core::str::Split<'a, &'a str>),
    Err(PQTMModuleError),
}

/// Parses the status field from a PQTM sentence.
/// The first part is expected to be the status ("OK" or "ERR").
/// This avoids code duplication in the higher level.
/// Returns a StatusField enum containing the status and the remaining parts.
pub fn parse_status_and_rest<'a>(
    mut parts: core::str::Split<'a, &'a str>,
) -> Result<StatusField<'a>, ParseError> {
    let status = parts.next().ok_or(ParseError::NoStatusField)?;

    match status {
        "OK" => Ok(StatusField::Ok(parts)),
        "ERROR" => {
            let code_str = parts.next().ok_or(ParseError::NoErrorCode)?;
            let code: u8 = code_str.parse().map_err(|_| ParseError::NoErrorCode)?;
            let err = match code {
                1 => PQTMModuleError::InvalidParameters,
                2 => PQTMModuleError::ExecutionFailed,
                _ => PQTMModuleError::Unknown(code),
            };
            Ok(StatusField::Err(err))
        }
        _ => Err(ParseError::InvalidStatusField),
    }
}

fn calc_checksum(input: &str) -> u8 {
    input.bytes().fold(0u8, |acc, i| acc ^ i)
}

pub fn wrap_sentence(payload: &str) -> String {
    let checksum = calc_checksum(payload);
    format!("${}*{:02X}\r\n", payload, checksum)
}

/// Unwraps a PQTM sentence, verifying its checksum and returning the payload if valid.
/// Example:
/// let sentence = "$PQTMVERNO,1.0,2.5,3*4A\r\n";
/// let payload = unwrap_sentence(sentence).unwrap();
/// assert_eq!(payload, "PQTMVERNO,1.0,2.5,3");
pub fn unwrap_sentence(sentence: &str) -> Result<&str, ParseError> {
    // Trim \r\n:
    let mut sentence = sentence.trim();
    sentence = sentence
        .strip_prefix("$")
        .ok_or(ParseError::StartDelimiterNotFound)?;
    let (payload, checksum_str) = sentence
        .split_once("*")
        .ok_or(ParseError::ChecksumNotFound)?;

    if checksum_str.len() != 2 {
        return Err(ParseError::ChecksumLengthInvalid);
    }

    let expected_checksum =
        u8::from_str_radix(checksum_str, 16).map_err(|_| ParseError::ChecksumLengthInvalid)?;

    if calc_checksum(payload) != expected_checksum {
        return Err(ParseError::ChecksumMismatch);
    }

    Ok(payload)
}
