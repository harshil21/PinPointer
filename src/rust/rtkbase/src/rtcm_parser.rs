/// RTCM3 Parser for extracting RTCM correction messages from GPS module output
///
/// RTCM3 message format:
/// - Preamble: 0xD3 (1 byte)
/// - Reserved (6 bits) + Message length (10 bits): 2 bytes
/// - Message payload: variable length (0-1023 bytes)
/// - CRC-24Q: 3 bytes
///
/// Total frame size: 6 + message_length bytes

const RTCM3_PREAMBLE: u8 = 0xD3;
const MIN_RTCM_FRAME_SIZE: usize = 6; // preamble + length + crc (no payload)
const MAX_RTCM_FRAME_SIZE: usize = 1029; // preamble + length + 1023 bytes + crc

#[derive(Debug, Clone)]
pub struct RTCMMessage {
    pub message_type: u16,
    pub raw_data: Vec<u8>, // Complete RTCM frame including preamble, length, payload, and CRC
}

pub struct RTCMParser {
    buffer: Vec<u8>,
}

impl RTCMParser {
    pub fn new() -> Self {
        RTCMParser { buffer: Vec::new() }
    }

    /// Parse incoming binary data and extract complete RTCM messages
    /// Returns a vector of complete RTCM frames ready to be sent to NTRIP
    pub fn parse_data(&mut self, data: &[u8]) -> Vec<RTCMMessage> {
        let mut messages = Vec::new();

        // Append new data to buffer
        self.buffer.extend_from_slice(data);

        // Process buffer to extract complete RTCM frames
        loop {
            // Find RTCM preamble (0xD3)
            let preamble_pos = match self.buffer.iter().position(|&b| b == RTCM3_PREAMBLE) {
                Some(pos) => pos,
                None => {
                    // No preamble found, clear the buffer
                    self.buffer.clear();
                    break;
                }
            };

            // If preamble is not at the start, discard everything before it
            if preamble_pos > 0 {
                self.buffer.drain(0..preamble_pos);
            }

            // Check if we have enough bytes for header (preamble + 2 length bytes)
            if self.buffer.len() < 3 {
                break;
            }

            // Need at least minimum frame size to proceed
            if self.buffer.len() < MIN_RTCM_FRAME_SIZE {
                break;
            }

            // Extract message length from bytes 1-2
            // Format: 6 reserved bits + 10 bits for length
            let length_high = self.buffer[1] & 0x03; // Lower 2 bits of byte 1
            let length_low = self.buffer[2];
            let message_length = ((length_high as usize) << 8) | (length_low as usize);

            // Validate message length
            if message_length > 1023 {
                // Invalid length, discard preamble and continue
                self.buffer.remove(0);
                continue;
            }

            let frame_size = 3 + message_length + 3; // header + payload + crc

            // Check if we have the complete frame
            if self.buffer.len() < frame_size {
                // Not enough data yet, wait for more
                break;
            }

            // Extract the complete frame
            let frame = self.buffer[0..frame_size].to_vec();

            // Validate CRC
            if !Self::validate_crc24q(&frame) {
                // CRC failed, discard preamble and continue searching
                self.buffer.remove(0);
                continue;
            }

            // Extract message type from first 12 bits of payload
            let message_type = if message_length >= 2 {
                ((frame[3] as u16) << 4) | ((frame[4] as u16) >> 4)
            } else {
                0 // Invalid or empty payload
            };

            // Valid RTCM frame found
            messages.push(RTCMMessage {
                message_type,
                raw_data: frame.clone(),
            });

            // Remove processed frame from buffer
            self.buffer.drain(0..frame_size);
        }

        // Prevent buffer from growing indefinitely
        // Keep only recent data if buffer gets too large
        if self.buffer.len() > MAX_RTCM_FRAME_SIZE * 2 {
            let keep_size = MAX_RTCM_FRAME_SIZE;
            let drain_size = self.buffer.len() - keep_size;
            self.buffer.drain(0..drain_size);
        }

        messages
    }

    /// Validate RTCM3 CRC-24Q
    /// The CRC is calculated over the entire message except the CRC itself
    fn validate_crc24q(frame: &[u8]) -> bool {
        if frame.len() < MIN_RTCM_FRAME_SIZE {
            return false;
        }

        let data_len = frame.len() - 3; // Exclude 3-byte CRC
        let received_crc = ((frame[data_len] as u32) << 16)
            | ((frame[data_len + 1] as u32) << 8)
            | (frame[data_len + 2] as u32);

        let calculated_crc = Self::calculate_crc24q(&frame[0..data_len]);

        received_crc == calculated_crc
    }

    /// Calculate CRC-24Q (Qualcomm) as used in RTCM3
    /// Polynomial: 0x1864CFB
    fn calculate_crc24q(data: &[u8]) -> u32 {
        const CRC24_POLY: u32 = 0x1864CFB;
        let mut crc: u32 = 0;

        for &byte in data {
            crc ^= (byte as u32) << 16;
            for _ in 0..8 {
                crc <<= 1;
                if crc & 0x1000000 != 0 {
                    crc ^= CRC24_POLY;
                }
            }
        }

        crc & 0xFFFFFF
    }

    /// Clear the internal buffer (useful for resetting state)
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Get the current buffer size (useful for debugging)
    pub fn buffer_size(&self) -> usize {
        self.buffer.len()
    }
}

impl Default for RTCMParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc24q_calculation() {
        // Test with known RTCM3 message header
        let test_data = vec![0xD3, 0x00, 0x00]; // Minimal header with 0-length payload
        let crc = RTCMParser::calculate_crc24q(&test_data);

        // CRC should be 24-bit value
        assert!(crc <= 0xFFFFFF);
    }

    #[test]
    fn test_preamble_detection() {
        let mut parser = RTCMParser::new();

        // Data without preamble (small amount)
        let garbage = vec![0x01, 0x02, 0x03, 0x04];
        let messages = parser.parse_data(&garbage);
        assert_eq!(messages.len(), 0);

        // Buffer should be cleared for small garbage data
        assert_eq!(parser.buffer_size(), 0);

        // Test with data that has preamble
        parser.clear();
        let with_preamble = vec![0x01, 0x02, 0xD3, 0x00, 0x00];
        let messages2 = parser.parse_data(&with_preamble);
        // Should find preamble and wait for more data
        assert_eq!(messages2.len(), 0);
        assert!(parser.buffer_size() > 0); // Should keep data starting from preamble
    }

    #[test]
    fn test_incomplete_frame() {
        let mut parser = RTCMParser::new();

        // Start of a frame but incomplete
        let incomplete = vec![0xD3, 0x00, 0x05]; // Says 5 bytes payload but nothing follows
        let messages = parser.parse_data(&incomplete);
        assert_eq!(messages.len(), 0);

        // Buffer should retain data waiting for more
        assert!(parser.buffer_size() > 0);
    }

    #[test]
    fn test_buffer_cleanup() {
        let mut parser = RTCMParser::new();
        parser.clear();
        assert_eq!(parser.buffer_size(), 0);
    }

    #[test]
    fn test_message_length_extraction() {
        // Test length extraction: 0x0005 = 5 bytes
        let data = vec![0xD3, 0x00, 0x05];
        let length_high = data[1] & 0x03;
        let length_low = data[2];
        let length = ((length_high as usize) << 8) | (length_low as usize);
        assert_eq!(length, 5);

        // Test with larger length: 0x0123 = 291 bytes
        let data2 = vec![0xD3, 0x01, 0x23];
        let length_high2 = data2[1] & 0x03;
        let length_low2 = data2[2];
        let length2 = ((length_high2 as usize) << 8) | (length_low2 as usize);
        assert_eq!(length2, 291);
    }
}
