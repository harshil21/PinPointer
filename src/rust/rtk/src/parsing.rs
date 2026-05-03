use crate::protocol::nmea::GgaData;
use crate::protocol::pair::PairResponse;
use crate::protocol::response::{PQTMResponse, WireMessage};
use crate::protocol::sentence::Deserialize;

pub struct PQTMParser {
    incomplete_sentence: String,
}

impl PQTMParser {
    pub fn new() -> Self {
        PQTMParser {
            incomplete_sentence: String::new(),
        }
    }

    /// Parses incoming data for complete NMEA sentences ($PQTM*, $PAIR*, $xxGGA).
    pub fn parse_data(&mut self, data: &str) -> Vec<WireMessage> {
        let mut outputs: Vec<WireMessage> = Vec::new();
        let mut buffer = self.incomplete_sentence.clone() + data;

        loop {
            // a. Find the next `$` in the buffer.
            let start_index = match buffer.find('$') {
                Some(index) => index,
                None => {
                    // b. No `$` found: discard buffer and stop.
                    self.incomplete_sentence.clear();
                    break;
                }
            };

            // c. Find `\r\n` after that `$`.
            let end_index = match buffer[start_index..].find("\r\n") {
                Some(index) => start_index + index + 2, // points just past `\r\n`
                None => {
                    // d. No terminator yet: stash the fragment and stop.
                    self.incomplete_sentence = buffer[start_index..].to_string();
                    break;
                }
            };

            // e. Extract the complete sentence (without the trailing `\r\n`).
            let sentence = &buffer[start_index..end_index - 2];

            // g. Dispatch by sentence type.
            if sentence.starts_with("$PQTM") {
                match PQTMResponse::from_sentence(sentence) {
                    Ok(resp) => {
                        outputs.push(WireMessage::PQTMMessage(resp));
                    }
                    Err(e) => {
                        eprintln!(
                            "Failed to parse PQTM Response: {:?}, Error: {:?}",
                            sentence, e
                        );
                    }
                }
            } else if sentence.starts_with("$PAIR") {
                match PairResponse::from_sentence(sentence) {
                    Ok(pair) => {
                        outputs.push(WireMessage::PairMessage(pair));
                    }
                    Err(e) => {
                        eprintln!(
                            "Failed to parse PAIR Message: {:?}, Error: {:?}",
                            sentence, e
                        );
                    }
                }
            } else if sentence.get(3..6) == Some("GGA") {
                if let Some(gga) = GgaData::parse(sentence) {
                    outputs.push(WireMessage::NmeaGga(gga));
                }
            }
            // else: silently skip unrecognised sentences.

            // f. Advance the buffer past `\r\n`.
            buffer = buffer[end_index..].to_string();
        }

        // Step 3: safety net – if no `$` remains in the buffer, make sure
        // incomplete_sentence is clear so stale data is not carried forward.
        if !buffer.contains('$') {
            self.incomplete_sentence.clear();
        }

        outputs
    }
}
