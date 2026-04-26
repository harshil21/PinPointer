/// This file is just for testing the rtkbase crate.
///
use rtk;
use rtk::port::BaseGPS;
use rtk::protocol::pair::{PairRTCMSetOutputMode, RtcmMode};
use rtk::protocol::response::WireMessage;
use std::path::PathBuf;

fn test_reading_sentences() {
    let mut rtk = BaseGPS::open_port(PathBuf::from("/dev/ttyUSB0")).unwrap();
    let _ = rtk.start();
    println!("Opened RTK GPS port successfully.");
    let timeout = std::time::Duration::from_secs(2);

    let mut count = 0;
    while count < 5 {
        if let Some(msg) = rtk.get_gps_data(timeout) {
            match &msg {
                WireMessage::PQTMMessage(resp) => {
                    println!("Received PQTM Response: {:?}", resp);
                }
                WireMessage::PairMessage(pair) => {
                    println!("Received PAIR Message: {:?}", pair);
                }
                WireMessage::NmeaGga(gga) => {
                    println!("Received GGA: {:?}", gga);
                }
            }
        }
        count += 1;
    }

    let ver_no = rtk.verno(timeout).unwrap();
    println!("Module Version: {:?}", ver_no.version);

    let rtcm_output_mode = rtk.pair_get_rtcm_mode(timeout).unwrap();
    println!("Current RTCM Output Mode: {:?}", rtcm_output_mode);
}

fn test_rtcm_parsing() {
    println!("\n=== Testing RTCM Parsing ===");
    let mut rtk = BaseGPS::open_port(PathBuf::from("/dev/ttyUSB0")).unwrap();
    let _ = rtk.start();
    let timeout = std::time::Duration::from_secs(3);

    // Enable RTCM output
    println!("Enabling RTCM3 MSM7 output...");
    let enable_rtcm = PairRTCMSetOutputMode {
        mode: RtcmMode::Rtcm3Msm7,
    };

    match rtk.pair_set_rtcm_mode(enable_rtcm, timeout) {
        Ok(_) => println!("✓ RTCM output enabled"),
        Err(e) => println!("Failed to enable RTCM: {:?}", e),
    }

    // Read RTCM messages
    println!("\nReading RTCM messages...");
    for i in 0..5 {
        if let Some(rtcm_msg) = rtk.get_rtcm_data(timeout) {
            println!(
                "RTCM #{}: Type {}, Size {} bytes",
                i + 1,
                rtcm_msg.message_type,
                rtcm_msg.raw_data.len()
            );

            // This raw_data can be sent to NTRIP caster
            // Example: upload_to_rtk2go(&rtcm_msg.raw_data);
        } else {
            println!("No RTCM data (may need RTK fix)");
        }
    }
}

fn main() {
    test_reading_sentences();
    test_rtcm_parsing();
}
