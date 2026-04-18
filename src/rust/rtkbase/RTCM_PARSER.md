# RTCM Parser Documentation

This document describes the RTCM parser functionality in the `rtkbase` crate, which extracts RTCM3 correction messages from GPS module output for uploading to NTRIP casters like RTK2Go.

## Overview

The RTCM parser handles binary RTCM3 (Radio Technical Commission for Maritime Services) messages that are output by RTK base station GPS modules. These corrections can be uploaded to an NTRIP caster to provide real-time kinematic (RTK) corrections to rover devices.

## RTCM3 Message Format

RTCM3 messages use a binary format with the following structure:

```
| Preamble | Reserved + Length | Message Payload | CRC-24Q |
|  1 byte  |      2 bytes      |  0-1023 bytes   | 3 bytes |
```

- **Preamble**: Always `0xD3`
- **Reserved + Length**: 6 reserved bits + 10-bit message length (0-1023 bytes)
- **Message Payload**: Variable length data containing the actual RTCM message
- **CRC-24Q**: 24-bit Qualcomm CRC for message validation

Total frame size: 6 + message_length bytes (minimum 6 bytes)

## Usage

### 1. Basic Setup

```rust
use rtkbase::port::BaseGPS;
use rtkbase::protocol::pair::{PairRTCMSetOutputMode, RtcmMode};
use std::path::PathBuf;
use std::time::Duration;

// Open GPS port
let mut rtk = BaseGPS::open_port(PathBuf::from("/dev/ttyUSB0"))?;

// Start the reader thread (handles both NMEA and RTCM parsing)
rtk.start();
```

### 2. Enable RTCM Output

Before you can receive RTCM messages, you need to enable RTCM output on the GPS module:

```rust
let timeout = Duration::from_secs(5);

// Enable RTCM3 MSM7 output (high precision)
let enable_rtcm = PairRTCMSetOutputMode {
    mode: RtcmMode::Rtcm3Msm7,
};

rtk.pair_set_rtcm_mode(enable_rtcm, timeout)?;
```

**Available RTCM Modes:**
- `RtcmMode::Disable` - Disable RTCM output
- `RtcmMode::Rtcm3Msm4` - RTCM3 MSM4 format (standard precision)
- `RtcmMode::Rtcm3Msm7` - RTCM3 MSM7 format (high precision, recommended)

### 3. Read RTCM Messages

```rust
// Blocking read with timeout
if let Some(rtcm_msg) = rtk.get_rtcm_data(Duration::from_secs(2)) {
    println!("Message Type: {}", rtcm_msg.message_type);
    println!("Frame Size: {} bytes", rtcm_msg.raw_data.len());
    
    // The raw_data contains the complete RTCM frame ready to upload
    upload_to_ntrip_caster(&rtcm_msg.raw_data);
}

// Non-blocking read
if let Some(rtcm_msg) = rtk.try_get_rtcm_data() {
    // Process message
}
```

### 4. Upload to NTRIP Caster

The `raw_data` field contains the complete RTCM frame (including preamble, length, payload, and CRC) ready to be sent to an NTRIP caster:

```rust
fn upload_to_ntrip_caster(rtcm_data: &[u8]) {
    // Connect to RTK2Go or another NTRIP caster
    // Send the raw RTCM data
    stream.write_all(rtcm_data)?;
}
```

## RTCMMessage Structure

```rust
pub struct RTCMMessage {
    pub message_type: u16,    // RTCM message type (e.g., 1005, 1077, 1087)
    pub raw_data: Vec<u8>,    // Complete RTCM frame (preamble + header + payload + CRC)
}
```

## Common RTCM Message Types

When running as a base station, you'll typically see these message types:

| Type | Description |
|------|-------------|
| 1005 | Stationary RTK Reference Station ARP (Antenna Reference Point) |
| 1006 | Stationary RTK Reference Station ARP with Height |
| 1019 | GPS Ephemerides |
| 1020 | GLONASS Ephemerides |
| 1033 | Receiver and Antenna Descriptors |
| 1074 | GPS MSM4 (Multi-Signal Message) |
| 1075 | GPS MSM5 |
| 1077 | GPS MSM7 (Full Carrier Phase) - High Precision |
| 1084 | GLONASS MSM4 |
| 1087 | GLONASS MSM7 |
| 1094 | Galileo MSM4 |
| 1097 | Galileo MSM7 |
| 1124 | BeiDou MSM4 |
| 1127 | BeiDou MSM7 |

**MSM4 vs MSM7:**
- MSM4: Standard precision, smaller messages
- MSM7: High precision, full carrier phase data, larger messages (recommended for RTK)

## Parser Features

### Automatic Frame Detection
The parser automatically:
- Searches for RTCM preambles (0xD3) in the incoming byte stream
- Extracts message length from the header
- Buffers incomplete frames until complete
- Validates CRC-24Q checksums
- Discards corrupted or invalid frames

### Buffer Management
- Handles mixed NMEA (text) and RTCM (binary) data in the same stream
- Prevents buffer overflow with automatic cleanup
- Maintains frame boundaries across multiple reads

### Validation
- CRC-24Q validation using the Qualcomm polynomial (0x1864CFB)
- Message length validation (0-1023 bytes)
- Frame structure validation

## Example: Complete Base Station

```rust
use rtkbase::port::BaseGPS;
use rtkbase::protocol::pair::{PairRTCMSetOutputMode, RtcmMode};
use std::path::PathBuf;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open GPS and start reading
    let mut rtk = BaseGPS::open_port(PathBuf::from("/dev/ttyUSB0"))?;
    rtk.start();
    
    let timeout = Duration::from_secs(5);
    
    // Enable RTCM output
    let enable_rtcm = PairRTCMSetOutputMode {
        mode: RtcmMode::Rtcm3Msm7,
    };
    rtk.pair_set_rtcm_mode(enable_rtcm, timeout)?;
    
    // Read and upload RTCM messages
    loop {
        if let Some(rtcm_msg) = rtk.get_rtcm_data(Duration::from_secs(2)) {
            println!("Uploading RTCM Type {} ({} bytes)",
                     rtcm_msg.message_type,
                     rtcm_msg.raw_data.len());
            
            // Upload to your NTRIP caster
            upload_to_rtk2go(&rtcm_msg.raw_data)?;
        }
    }
}
```

## Testing

Run the included tests:

```bash
cargo test -p rtkbase --lib rtcm_parser
```

Run the example:

```bash
cargo run --example rtcm_parser_example
```

## Integration with NTRIP

To create a complete RTK base station that uploads to RTK2Go:

1. Enable RTCM output on your GPS module (MSM7 recommended)
2. Read RTCM messages using `get_rtcm_data()`
3. Connect to RTK2Go NTRIP caster as a source
4. Upload the `raw_data` from each `RTCMMessage`

See the `ntrip` crate for NTRIP client/server functionality.

## Requirements

- GPS module capable of outputting RTCM3 corrections (e.g., LC29H-BS)
- GPS module must be in base station mode with survey-in complete
- Serial connection to GPS module (typically 115200 baud)

## Notes

- RTCM messages are only output when the base station has a valid RTK fix
- The GPS module must complete its survey-in process before generating corrections
- MSM7 messages are larger but provide better accuracy than MSM4
- You can monitor both NMEA status messages and RTCM corrections simultaneously

## Troubleshooting

**No RTCM messages received:**
- Check if RTCM output mode is enabled
- Verify the GPS module has completed survey-in (check SVIN status)
- Ensure the GPS module has a valid fix
- Check that RTCM mode is set to MSM4 or MSM7, not Disabled

**CRC validation failures:**
- May indicate serial communication issues
- Check baud rate (should be 115200)
- Verify cable quality and connections

**Buffer growing indefinitely:**
- The parser automatically manages buffer size
- Maximum buffer size is capped at 2× max frame size
- Old data is discarded if buffer grows too large