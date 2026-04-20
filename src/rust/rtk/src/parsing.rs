use crate::protocol::{pair::{PairResponse}, response::{PQTMResponse, WireMessage}, sentence::Deserialize};

pub struct PQTMParser {
    incomplete_sentence: String,
}

impl PQTMParser {
    pub fn new() -> Self {
        PQTMParser {
            incomplete_sentence: String::new(),
        }
    }

    /// Parses incoming data for complete $PQTM* sentences.
    pub fn parse_data(&mut self, data: &str) -> Vec<WireMessage> {
        let mut complete_parsed_sentences: Vec<String> = Vec::new();
        let mut buffer = self.incomplete_sentence.clone() + data;

        // Loop to find complete sentences in the buffer. Break when the next sentence is
        // incomplete.
        loop {
            let start_index = match buffer.find("$P") {
                Some(index) => index,
                None => {
                    // No start found, discard buffer
                    self.incomplete_sentence.clear();
                    break;
                }
            };

            // Find the end of the sentence:
            let end_index = match buffer[start_index..].find("\r\n") {
                Some(index) => start_index + index + 2, // Include \r\n
                None => {
                    // No end found, store incomplete sentence
                    self.incomplete_sentence = buffer[start_index..].to_string();
                    break;
                }
            };

            // Extract complete sentence
            let complete_sentence = &buffer[start_index..end_index];
            println!("\n\nComplete PQTM sentence: {}", complete_sentence);
            complete_parsed_sentences.push(complete_sentence.to_string());

            // Move the buffer forward:
            buffer = buffer[end_index..].to_string();
        }

        let mut pqtm_outputs: Vec<WireMessage> = Vec::new();
        
        for s in &complete_parsed_sentences {
            if s.starts_with("$PQTM") {
                let resp = PQTMResponse::from_sentence(&s);
                match resp {
                    Err(e) => {
                        eprintln!("Failed to parse PQTM Response: {:?}, Error: {:?}", s, e);
                    }
                    Ok(resp) => {
                        pqtm_outputs.push(WireMessage::PQTMMessage(resp));
                    }
                }
            } else if s.starts_with("$PAIR") {
                let resp = PairResponse::from_sentence(&s);
                match resp {
                    Err(e) => {
                        eprintln!("Failed to parse PAIR Message: {:?}, Error: {:?}", s, e);
                    }
                    Ok(pair) => {
                        pqtm_outputs.push(WireMessage::PairMessage(pair));
                    }
                }
            }
        }
        
        pqtm_outputs
    }
}
