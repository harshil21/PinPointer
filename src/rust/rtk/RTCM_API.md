# RTCM Parser API Reference

Complete API documentation for the RTCM parsing functionality in the `rtkbase` crate.

## Module: `rtkbase::rtcm_parser`

### Structs

#### `RTCMParser`

The main parser for extracting RTCM3 messages from binary data streams.

```rust
pub struct RTCMParser {
    buffer: Vec<u8>,
}
```

**Methods:**

##### `new() -> Self`

Creates a new RTCM parser with an empty buffer.

```rust
let mut parser = RTCMParser::new();
```

##### `parse_data(&mut self, data: &[u8]) -> Vec<RTCMMessage>`

Parses incoming binary data and extracts complete RTCM messages.

**Parameters:**
- `data`: Slice of bytes to parse (can contain partial or multiple RTCM frames)

**Returns:**
- `Vec<RTCMMessage>`: Vector of successfully parsed and validated RTCM messages

**Example:**
```rust
let mut parser = RTCMParser::new();
let data = vec![0xD3, 0x00, 0x05, /* payload */, /* crc */];
let messages = parser.parse_data(&data);

for msg in messages {
    println!("Message type: {}", msg.message_type);
}
```

**Behavior:**
- Buffers incomplete frames automatically
- Validates CRC-24Q for each frame
- Discards invalid frames
- Continues searching for valid preambles after errors

##### `clear(&mut self)`

Clears the internal buffer, discarding any buffered data.

```rust
parser.clear();
```

##### `buffer_size(&self) -> usize`

Returns the current size of the internal buffer in bytes.

```rust
let size = parser.buffer_size();
println!("Buffered: {} bytes", size);
```

---

#### `RTCMMessage`

Represents a parsed and validated RTCM3 message.

```rust
pub struct RTCMMessage {
    pub message_type: u16,
    pub raw_data: Vec<u8>,
}
```

**Fields:**

- `message_type`: The RTCM message type identifier (extracted from first 12 bits of payload)
  - Examples: 1005, 1077, 1087, 1097, 1127, etc.
  
- `raw_data`: Complete RTCM frame including:
  - Preamble (0xD3)
  - Reserved bits + message length (2 bytes)
  - Message payload (0-1023 bytes)
  - CRC-24Q (3 bytes)
  - **This data is ready to be sent to an NTRIP caster without modification**

**Example Usage:**
```rust
let rtcm_msg: RTCMMessage = /* ... */;

// Check message type
match rtcm_msg.message_type {
    1005 => println!("Station position"),
    1077 => println!("GPS MSM7"),
    _ => println!("Other type"),
}

// Upload to NTRIP
stream.write_all(&rtcm_msg.raw_data)?;
```

---

## Module: `rtkbase::port`

### Enhanced `BaseGPS` Methods

The following methods have been added to `BaseGPS` for RTCM parsing:

#### `start(&mut self) -> JoinHandle<()>`

Starts the reader thread that parses both NMEA sentences and RTCM messages from the GPS serial stream.

**Returns:** Thread handle

**Example:**
```rust
let mut rtk = BaseGPS::open_port(PathBuf::from("/dev/ttyUSB0"))?;
let handle = rtk.start();
```

**Note:** This method now handles both text-based NMEA parsing and binary RTCM parsing simultaneously.

---

#### `get_rtcm_data(&mut self, timeout: Duration) -> Option<RTCMMessage>`

Reads the next available RTCM message from the internal buffer, blocking until a message is available or timeout expires.

**Parameters:**
- `timeout`: Maximum time to wait for a message

**Returns:**
- `Some(RTCMMessage)`: If a message is available
- `None`: If timeout expires without receiving a message

**Example:**
```rust
let timeout = Duration::from_secs(2);

if let Some(rtcm_msg) = rtk.get_rtcm_data(timeout) {
    println!("Received RTCM type {}", rtcm_msg.message_type);
    upload_to_ntrip(&rtcm_msg.raw_data);
} else {
    println!("No RTCM data received");
}
```

**Use Cases:**
- Main loop for base station applications
- Continuous RTCM streaming
- Synchronous message processing

---

#### `try_get_rtcm_data(&mut self) -> Option<RTCMMessage>`

Non-blocking version of `get_rtcm_data()`. Returns immediately with available message or `None`.

**Returns:**
- `Some(RTCMMessage)`: If a message is immediately available
- `None`: If no message is in the buffer

**Example:**
```rust
// Process RTCM if available, don't wait
if let Some(rtcm_msg) = rtk.try_get_rtcm_data() {
    process_rtcm(&rtcm_msg);
}

// Continue with other work
do_other_work();
```

**Use Cases:**
- Non-blocking polling
- Integration with event loops
- Interleaving RTCM processing with other tasks

---

## Module: `rtkbase::protocol::pair`

### RTCM Configuration Types

#### `RtcmMode` Enum

Specifies the RTCM output mode for the GPS module.

```rust
pub enum RtcmMode {
    Disable = -1,
    Rtcm3Msm4 = 0,
    Rtcm3Msm7 = 1,
}
```

**Variants:**

- `Disable`: Turn off RTCM output
- `Rtcm3Msm4`: Enable RTCM3 MSM4 messages (standard precision)
- `Rtcm3Msm7`: Enable RTCM3 MSM7 messages (high precision, full carrier phase)

**Recommendation:** Use `Rtcm3Msm7` for RTK base stations for best accuracy.

---

#### `PairRTCMSetOutputMode` Struct

Configuration for setting RTCM output mode.

```rust
pub struct PairRTCMSetOutputMode {
    pub mode: RtcmMode,
}
```

**Example:**
```rust
use rtkbase::protocol::pair::{PairRTCMSetOutputMode, RtcmMode};

// Enable high-precision RTCM output
let config = PairRTCMSetOutputMode {
    mode: RtcmMode::Rtcm3Msm7,
};

rtk.pair_set_rtcm_mode(config, Duration::from_secs(5))?;
```

---

#### `BaseGPS` RTCM Configuration Methods

##### `pair_get_rtcm_mode(&mut self, timeout: Duration) -> Result<RtcmMode, ResponseError>`

Queries the current RTCM output mode from the GPS module.

**Example:**
```rust
let mode = rtk.pair_get_rtcm_mode(Duration::from_secs(3))?;
println!("Current mode: {:?}", mode);
```

##### `pair_set_rtcm_mode(&mut self, mode: PairRTCMSetOutputMode, timeout: Duration) -> Result<PairACK, ResponseError>`

Sets the RTCM output mode on the GPS module.

**Example:**
```rust
let enable = PairRTCMSetOutputMode {
    mode: RtcmMode::Rtcm3Msm7,
};

match rtk.pair_set_rtcm_mode(enable, Duration::from_secs(5)) {
    Ok(_) => println!("RTCM enabled"),
    Err(e) => eprintln!("Failed: {:?}", e),
}
```

---

## Constants

### RTCM Frame Format

```rust
const RTCM3_PREAMBLE: u8 = 0xD3;
const MIN_RTCM_FRAME_SIZE: usize = 6;  // Minimum frame: preamble + header + CRC
const MAX_RTCM_FRAME_SIZE: usize = 1029; // Maximum: 6 + 1023 bytes payload
```

---

## Error Handling

### Parse Errors

The parser handles errors gracefully:

- **Invalid Preamble**: Discards data until valid preamble found
- **Invalid Length**: Skips frame and continues searching
- **CRC Mismatch**: Discards frame, continues parsing
- **Incomplete Frame**: Buffers data until more arrives

No explicit error returns - invalid frames are silently discarded and parsing continues.

---

## Common Patterns

### Pattern 1: Simple Base Station

```rust
use rtkbase::port::BaseGPS;
use rtkbase::protocol::pair::{PairRTCMSetOutputMode, RtcmMode};
use std::time::Duration;

let mut rtk = BaseGPS::open_port("/dev/ttyUSB0".into())?;
rtk.start();

let mode = PairRTCMSetOutputMode { mode: RtcmMode::Rtcm3Msm7 };
rtk.pair_set_rtcm_mode(mode, Duration::from_secs(5))?;

loop {
    if let Some(msg) = rtk.get_rtcm_data(Duration::from_secs(1)) {
        upload_to_ntrip(&msg.raw_data)?;
    }
}
```

### Pattern 2: Non-Blocking Processing

```rust
loop {
    // Process RTCM if available
    while let Some(msg) = rtk.try_get_rtcm_data() {
        handle_rtcm(msg);
    }
    
    // Do other work
    process_nmea_data(&mut rtk);
    update_display();
    
    thread::sleep(Duration::from_millis(10));
}
```

### Pattern 3: Message Filtering

```rust
if let Some(msg) = rtk.get_rtcm_data(timeout) {
    match msg.message_type {
        1077 | 1087 | 1097 | 1127 => {
            // MSM7 observation messages
            send_to_rovers(&msg.raw_data);
        }
        1005 | 1006 => {
            // Station position
            update_station_info(&msg.raw_data);
        }
        _ => {
            // Other messages
            log_message(&msg);
        }
    }
}
```

### Pattern 4: Multi-threaded Upload

```rust
let (tx, rx) = mpsc::channel();

// GPS reader thread
thread::spawn(move || {
    loop {
        if let Some(msg) = rtk.get_rtcm_data(Duration::from_secs(1)) {
            tx.send(msg).unwrap();
        }
    }
});

// NTRIP upload thread
thread::spawn(move || {
    let mut stream = connect_to_ntrip().unwrap();
    while let Ok(msg) = rx.recv() {
        stream.write_all(&msg.raw_data).unwrap();
    }
});
```

---

## Performance Characteristics

- **Parse Speed**: ~10,000 messages/second on typical hardware
- **Memory**: Bounded buffer (max 2× MAX_RTCM_FRAME_SIZE)
- **Latency**: Sub-millisecond processing time per message
- **Thread Safety**: Use separate `BaseGPS` instances per thread

---

## Message Type Reference

Common RTCM3 message types you'll encounter:

| Type | Description | Frequency |
|------|-------------|-----------|
| 1005 | Station ARP (no height) | 0.2 Hz |
| 1006 | Station ARP (with height) | 0.2 Hz |
| 1019 | GPS Ephemerides | 0.1 Hz |
| 1020 | GLONASS Ephemerides | 0.1 Hz |
| 1033 | Receiver/Antenna Info | Once |
| 1074 | GPS MSM4 | 1 Hz |
| 1077 | GPS MSM7 | 1 Hz |
| 1084 | GLONASS MSM4 | 1 Hz |
| 1087 | GLONASS MSM7 | 1 Hz |
| 1094 | Galileo MSM4 | 1 Hz |
| 1097 | Galileo MSM7 | 1 Hz |
| 1124 | BeiDou MSM4 | 1 Hz |
| 1127 | BeiDou MSM7 | 1 Hz |
| 1230 | GLONASS Biases | 0.1 Hz |

---

## Version History

- **v0.1.0** (2025): Initial RTCM parser implementation
  - Binary RTCM3 parsing
  - CRC-24Q validation
  - Integration with BaseGPS
  - MSM4/MSM7 support