use crate::dispatcher::Dispatcher;
use crate::parsing::PQTMParser;
use crate::protocol::commands::PQTMCommand;
use crate::protocol::pair::{AckResult, PairACK, PairCommand, PairResponse};
use crate::protocol::response::PQTMResponse;
use crate::protocol::response::ParseError;
use crate::protocol::response::ResponseError;
use crate::protocol::response::WireMessage;
use crate::protocol::sentence::Serialize;
use crate::rtcm_parser::{RTCMMessage, RTCMParser};
use serialport::Error;
use serialport::TTYPort;
use std::io::BufReader;
use std::io::Read;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

pub struct BaseGPS {
    base_gps_port: TTYPort,
    stream_rx: Option<Receiver<WireMessage>>,
    rtcm_rx: Option<Receiver<RTCMMessage>>,
    dispatcher: Dispatcher,
    stop_signal: Arc<AtomicBool>,
}

impl BaseGPS {
    const BAUD_RATE: u32 = 115_200;

    /// Starts a thread to read data from the GPS port, extracts complete NMEA sentences and RTCM messages.
    pub fn start(&mut self) -> JoinHandle<()> {
        let (stream_tx, stream_rx) = mpsc::channel();
        let (rtcm_tx, rtcm_rx) = mpsc::channel();
        self.stream_rx = Some(stream_rx);
        self.rtcm_rx = Some(rtcm_rx);
        self.dispatcher.set_stream_tx(stream_tx);
        self.rtk_reader_thread(rtcm_tx)
    }

    /// Pops the next available PqtmOutput from the internal buffer, if any.
    pub fn get_gps_data(&mut self, timeout: Duration) -> Option<WireMessage> {
        // println!("Checking for GPS data (in get_gps_data)...");
        self.stream_rx.as_ref()?.recv_timeout(timeout).ok()
    }

    /// Non-blocking: returns the next GPS data message if one is available,
    /// or None if the queue is empty.
    pub fn try_get_gps_data(&mut self) -> Option<WireMessage> {
        self.stream_rx.as_ref()?.try_recv().ok()
    }

    /// Pops the next available RTCM message from the internal buffer, if any.
    /// Use this to get RTCM correction data that can be uploaded to NTRIP caster.
    pub fn get_rtcm_data(&mut self, timeout: Duration) -> Option<RTCMMessage> {
        self.rtcm_rx.as_ref()?.recv_timeout(timeout).ok()
    }

    /// Tries to get an RTCM message without blocking.
    /// Returns None if no message is available.
    pub fn try_get_rtcm_data(&mut self) -> Option<RTCMMessage> {
        self.rtcm_rx.as_ref()?.try_recv().ok()
    }

    pub fn open_port(port: PathBuf) -> Result<BaseGPS, Error> {
        match serialport::new(port.to_string_lossy(), Self::BAUD_RATE)
            .timeout(std::time::Duration::from_millis(5000))
            .open_native()
        {
            Ok(base_gps_port) => {
                println!("Successfully opened port {}", port.to_string_lossy());
                Ok(BaseGPS {
                    base_gps_port,
                    stream_rx: None,
                    rtcm_rx: None,
                    dispatcher: Dispatcher::new(),
                    stop_signal: Arc::new(AtomicBool::new(false)),
                })
            }
            Err(e) => {
                eprintln!(
                    "Failed to open \"{}\". Error: {}",
                    port.to_string_lossy(),
                    e
                );
                Err(e)
            }
        }
    }

    pub fn send_command(
        &mut self,
        command: PQTMCommand,
        timeout: Duration,
    ) -> Result<PQTMResponse, ResponseError> {
        let (wait_tx, wait_rx) = mpsc::channel();

        // Register a waiter for the expected response:

        self.dispatcher.register_waiter(
            Box::new(|m| match m {
                WireMessage::PQTMMessage(PQTMResponse::Epe(_)) => false,
                WireMessage::PQTMMessage(PQTMResponse::SvinStatus(_)) => false,
                WireMessage::PQTMMessage(_) => true,
                _ => false,
            }),
            wait_tx,
            1,
        );

        // Send command:
        let sentence = command.to_sentence();
        self.write_all(sentence.as_bytes()).map_err(|_| {
            ResponseError::ParseError(ParseError::ParsingError("writing to GPS port failed"))
        })?;

        // Wait for response:
        match wait_rx.recv_timeout(timeout) {
            Ok(WireMessage::PQTMMessage(resp)) => Ok(resp),
            Ok(_) => Err(ResponseError::ParseError(ParseError::ParsingError(
                "unexpected message type received",
            ))),
            Err(_) => Err(ResponseError::ParseError(ParseError::ParsingError(
                "timeout waiting for response",
            ))),
        }
    }

    /// Sends a PAIR get command (e.g., PAIR433, PAIR435).
    /// Waits for ACK, then waits for the actual response.
    /// Returns both so you can validate the ACK and get the data.
    pub fn send_pair_get(
        &mut self,
        command: PairCommand,
        timeout: Duration,
    ) -> Result<(PairACK, PairResponse), ResponseError> {
        let (wait_tx, wait_rx) = mpsc::channel();

        // Register ONCE for any PAIR message
        self.dispatcher.register_waiter(
            Box::new(|m| matches!(m, WireMessage::PairMessage(_))),
            wait_tx,
            2,
        );

        let sentence = command.to_sentence();
        self.write_all(sentence.as_bytes())
            .map_err(|_| ResponseError::ParseError(ParseError::ParsingError("write failed")))?;

        // Wait for ACK first
        let ack = match wait_rx.recv_timeout(timeout) {
            Ok(WireMessage::PairMessage(PairResponse::ACK(ack))) => {
                if ack.result != AckResult::Success {
                    return Err(ResponseError::ParseError(ParseError::ParsingError(
                        "ACK failed",
                    )));
                }
                ack
            }
            Ok(_) => {
                return Err(ResponseError::ParseError(ParseError::ParsingError(
                    "expected ACK, got something else",
                )));
            }
            Err(_) => {
                return Err(ResponseError::ParseError(ParseError::ParsingError(
                    "timeout waiting for ACK",
                )));
            }
        };

        // Now wait for the actual response (same waiter, same channel)
        match wait_rx.recv_timeout(timeout) {
            Ok(WireMessage::PairMessage(resp)) => Ok((ack, resp)),
            Ok(_) => Err(ResponseError::ParseError(ParseError::ParsingError(
                "unexpected message type",
            ))),
            Err(_) => Err(ResponseError::ParseError(ParseError::ParsingError(
                "timeout waiting for response",
            ))),
        }
    }

    pub fn send_pair_set(
        &mut self,
        command: PairCommand,
        timeout: Duration,
    ) -> Result<PairACK, ResponseError> {
        let (wait_tx, wait_rx) = mpsc::channel();

        // Wait for ACK only
        self.dispatcher.register_waiter(
            Box::new(|m| matches!(m, WireMessage::PairMessage(PairResponse::ACK(_)))),
            wait_tx,
            1,
        );

        let sentence = command.to_sentence();
        self.write_all(sentence.as_bytes())
            .map_err(|_| ResponseError::ParseError(ParseError::ParsingError("write failed")))?;

        match wait_rx.recv_timeout(timeout) {
            Ok(WireMessage::PairMessage(PairResponse::ACK(ack))) => {
                if ack.result == AckResult::Success {
                    Ok(ack)
                } else {
                    // ACK failed - return error with the ACK result embedded
                    Err(ResponseError::ParseError(ParseError::ParsingError(
                        "ACK failed",
                    )))
                }
            }
            Ok(_) => Err(ResponseError::ParseError(ParseError::ParsingError(
                "unexpected message",
            ))),
            Err(_) => Err(ResponseError::ParseError(ParseError::ParsingError(
                "timeout",
            ))),
        }
    }

    fn rtk_reader_thread(&self, rtcm_tx: mpsc::Sender<RTCMMessage>) -> JoinHandle<()> {
        let mut reader = BufReader::new(
            self.base_gps_port
                .try_clone_native()
                .expect("Failed to clone GPS port"),
        );
        let mut serial_buf: Vec<u8> = vec![0; 512];
        let stop_signal = self.stop_signal.clone();
        let mut nmea_parser = PQTMParser::new();
        let mut rtcm_parser = RTCMParser::new();
        let dispatcher = self.dispatcher.clone();

        thread::spawn(move || {
            while !stop_signal.load(Ordering::Acquire) {
                match reader.read(&mut serial_buf) {
                    Ok(t) if t > 0 => {
                        // Parse NMEA sentences (text-based)
                        let chunk = String::from_utf8_lossy(&serial_buf[..t]).into_owned();
                        for msg in nmea_parser.parse_data(&chunk) {
                            // println!("Parsed GPS message: {:?}", msg);
                            dispatcher.dispatch(msg);
                        }

                        // Parse RTCM messages (binary)
                        for rtcm_msg in rtcm_parser.parse_data(&serial_buf[..t]) {
                            // println!("Parsed RTCM message type: {}", rtcm_msg.message_type);
                            let _ = rtcm_tx.send(rtcm_msg);
                        }
                    }

                    Ok(_) => continue, // No data read, continue
                    Err(e) => {
                        eprintln!("Error reading from GPS port: {}", e);
                        break;
                    }
                }
            }
        })
    }
}

impl Write for BaseGPS {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.base_gps_port.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.base_gps_port.flush()
    }
}
