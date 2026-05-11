use crate::protocol::nmea::{GgaData, GsvConstellation, GsvData, GsvSatellite};
use crate::protocol::pair::PairResponse;
use crate::protocol::response::{PQTMResponse, WireMessage};
use crate::protocol::sentence::Deserialize;

// ── GSV accumulator ───────────────────────────────────────────────────────────

/// Holds partial satellite data while accumulating a multi-sentence GSV sequence.
struct GsvAccumulator {
    /// Total number of messages expected in this sequence.
    total_msgs: u8,
    /// Satellites collected so far.
    satellites: Vec<GsvSatellite>,
    /// Which constellation this sequence is for (from the talker ID).
    constellation: GsvConstellation,
}

impl GsvAccumulator {
    fn new(total_msgs: u8, constellation: GsvConstellation) -> Self {
        Self {
            total_msgs,
            satellites: Vec::new(),
            constellation,
        }
    }
}

// ── Parser ────────────────────────────────────────────────────────────────────

pub struct PQTMParser {
    incomplete_sentence: String,
    /// Accumulates satellites across a multi-message GSV sequence.
    gsv_accumulator: Option<GsvAccumulator>,
}

impl PQTMParser {
    pub fn new() -> Self {
        PQTMParser {
            incomplete_sentence: String::new(),
            gsv_accumulator: None,
        }
    }

    /// Parses incoming data for complete NMEA sentences ($PQTM*, $PAIR*, $xxGGA, $xxGSV).
    pub fn parse_data(&mut self, data: &str) -> Vec<WireMessage> {
        let mut outputs: Vec<WireMessage> = Vec::new();
        let mut buffer = self.incomplete_sentence.clone() + data;

        loop {
            // a. Find the next `$` in the buffer.
            let start_index = match buffer.find('$') {
                Some(index) => index,
                None => {
                    self.incomplete_sentence.clear();
                    break;
                }
            };

            // c. Find `\r\n` after that `$`.
            let end_index = match buffer[start_index..].find("\r\n") {
                Some(index) => start_index + index + 2,
                None => {
                    self.incomplete_sentence = buffer[start_index..].to_string();
                    break;
                }
            };

            // e. Extract the complete sentence (without the trailing `\r\n`).
            let sentence = &buffer[start_index..end_index - 2];

            // g. Dispatch by sentence type.
            if sentence.starts_with("$PQTM") {
                log::debug!(
                    "[PARSER] PQTM sentence: {}",
                    &sentence[..sentence.len().min(80)]
                );
                match PQTMResponse::from_sentence(sentence) {
                    Ok(resp) => {
                        outputs.push(WireMessage::PQTMMessage(resp));
                    }
                    Err(e) => {
                        log::warn!(
                            "[PARSER] Failed to parse PQTM sentence '{}': {:?}",
                            &sentence[..sentence.len().min(80)],
                            e
                        );
                    }
                }
            } else if sentence.starts_with("$PAIR") {
                match PairResponse::from_sentence(sentence) {
                    Ok(pair) => {
                        outputs.push(WireMessage::PairMessage(pair));
                    }
                    Err(e) => {
                        log::warn!(
                            "[PARSER] Failed to parse PAIR sentence '{}': {:?}",
                            &sentence[..sentence.len().min(80)],
                            e
                        );
                    }
                }
            } else if sentence.get(3..6) == Some("GGA") {
                if let Some(gga) = GgaData::parse(sentence) {
                    outputs.push(WireMessage::NmeaGga(gga));
                }
            } else if sentence.get(3..6) == Some("GSV") {
                let constellation = sentence
                    .get(1..3)
                    .map(GsvConstellation::from_talker)
                    .unwrap_or(GsvConstellation::Unknown);
                log::debug!(
                    "[PARSER] GSV sentence arrived ({}): {}",
                    &sentence[..sentence.len().min(8)],
                    &sentence[..sentence.len().min(120)]
                );
                if let Some(gsv) = self.parse_gsv(sentence, constellation) {
                    log::debug!(
                        "[PARSER] GSV sequence complete: {} satellites",
                        gsv.satellites.len()
                    );
                    outputs.push(WireMessage::NmeaGsv(gsv));
                }
            }

            // f. Advance the buffer past `\r\n`.
            buffer = buffer[end_index..].to_string();
        }

        if !buffer.contains('$') {
            self.incomplete_sentence.clear();
        }

        outputs
    }

    // ── GSV parsing ───────────────────────────────────────────────────────────

    /// Parse a single GSV sentence and accumulate its satellites.
    ///
    /// Returns `Some(GsvData)` only when the final sentence in the sequence
    /// has been received (i.e. `msg_num == total_msgs`), so the caller gets
    /// the complete satellite set in one shot.
    fn parse_gsv(&mut self, sentence: &str, constellation: GsvConstellation) -> Option<GsvData> {
        // Strip checksum.
        let sentence = match sentence.find('*') {
            Some(idx) => &sentence[..idx],
            None => sentence,
        };

        let fields: Vec<&str> = sentence.split(',').collect();
        // Minimum: $??GSV, total_msgs, msg_num, num_sats  → 4 fields
        if fields.len() < 4 {
            return None;
        }

        let total_msgs: u8 = fields[1].parse().ok()?;
        let msg_num: u8 = fields[2].parse().ok()?;

        // Reset accumulator on the first message of a new sequence.
        if msg_num == 1 {
            self.gsv_accumulator = Some(GsvAccumulator::new(total_msgs, constellation));
        }

        // Extract satellite blocks from the remaining fields.
        // Each block is 4 fields: PRN, elevation, azimuth, SNR.
        // Fields start at index 4.
        let acc = self.gsv_accumulator.as_mut()?;
        let mut i = 4;
        while i + 3 < fields.len() {
            let prn_str = fields[i];
            let snr_str = fields[i + 3];

            if !prn_str.is_empty() {
                if let Ok(prn) = prn_str.parse::<u8>() {
                    let snr = if snr_str.is_empty() {
                        None
                    } else {
                        snr_str.parse::<u8>().ok()
                    };
                    acc.satellites.push(GsvSatellite { prn, snr });
                }
            }
            i += 4;
        }

        // Emit the complete set when the last message arrives.
        if msg_num == acc.total_msgs {
            let finished = self.gsv_accumulator.take()?;
            Some(GsvData {
                satellites: finished.satellites,
                constellation: finished.constellation,
            })
        } else {
            None
        }
    }
}
