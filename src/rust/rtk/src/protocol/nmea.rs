/// NMEA GGA sentence data and fix quality types.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpsFixQuality {
    NoFix = 0,
    GpsFix = 1,
    DgpsFix = 2,
    PpsFix = 3,
    RtkFixed = 4,
    RtkFloat = 5,
    DeadReckoning = 6,
    Unknown = 255,
}

impl From<u8> for GpsFixQuality {
    fn from(v: u8) -> Self {
        match v {
            0 => GpsFixQuality::NoFix,
            1 => GpsFixQuality::GpsFix,
            2 => GpsFixQuality::DgpsFix,
            3 => GpsFixQuality::PpsFix,
            4 => GpsFixQuality::RtkFixed,
            5 => GpsFixQuality::RtkFloat,
            6 => GpsFixQuality::DeadReckoning,
            _ => GpsFixQuality::Unknown,
        }
    }
}

/// Parsed NMEA GGA sentence.
#[derive(Debug, Clone)]
pub struct GgaData {
    pub latitude: f64,   // decimal degrees, positive = North
    pub longitude: f64,  // decimal degrees, positive = East
    pub altitude_m: f32, // metres above MSL
    pub fix_quality: GpsFixQuality,
    pub satellites_used: u8,
    pub hdop: f32,
}

/// Converts an NMEA DDmm.mmmm / DDDmm.mmmm value to decimal degrees.
///
/// * `value`         – the raw NMEA field string (e.g. `"4807.0383"`)
/// * `degree_digits` – number of leading characters that form the integer-degree
///                     part (2 for latitude, 3 for longitude)
fn nmea_to_decimal_degrees(value: &str, degree_digits: usize) -> Option<f64> {
    if value.len() <= degree_digits {
        return None;
    }
    let degrees: f64 = value[..degree_digits].parse().ok()?;
    let minutes: f64 = value[degree_digits..].parse().ok()?;
    Some(degrees + minutes / 60.0)
}

impl GgaData {
    /// Parse a `$GPGGA` / `$GNGGA` / `$GLGGA` / `$GAGGA` sentence.
    ///
    /// Returns `None` if the sentence is malformed or is not a GGA sentence.
    pub fn parse(sentence: &str) -> Option<Self> {
        // 1. Strip checksum (everything after and including `*`).
        let sentence = match sentence.find('*') {
            Some(idx) => &sentence[..idx],
            None => sentence,
        };

        // 2. Split on `,`.
        let fields: Vec<&str> = sentence.split(',').collect();

        // 3. Verify fields[0] ends with "GGA" (handles GPGGA, GNGGA, GLGGA, GAGGA, …).
        if !fields.first()?.ends_with("GGA") {
            return None;
        }

        // 4. Require at least 10 fields.
        if fields.len() < 10 {
            return None;
        }

        // 5a. Parse latitude (fields[2]) + N/S direction (fields[3]).
        let lat_raw = fields[2];
        let lat_dir = fields[3];
        if lat_raw.is_empty() {
            return None;
        }
        let lat = nmea_to_decimal_degrees(lat_raw, 2)?;
        let latitude = if lat_dir == "S" { -lat } else { lat };

        // 5b. Parse longitude (fields[4]) + E/W direction (fields[5]).
        let lon_raw = fields[4];
        let lon_dir = fields[5];
        if lon_raw.is_empty() {
            return None;
        }
        let lon = nmea_to_decimal_degrees(lon_raw, 3)?;
        let longitude = if lon_dir == "W" { -lon } else { lon };

        // 5c. Fix quality (fields[6]).
        let fix_quality = fields[6]
            .parse::<u8>()
            .map(GpsFixQuality::from)
            .unwrap_or(GpsFixQuality::Unknown);

        // 5d. Satellites used (fields[7]).
        let satellites_used = fields[7].parse::<u8>().unwrap_or(0);

        // 5e. HDOP (fields[8]).
        let hdop = fields[8].parse::<f32>().unwrap_or(0.0);

        // 5f. Altitude above MSL (fields[9]).
        let altitude_m = fields[9].parse::<f32>().unwrap_or(0.0);

        Some(GgaData {
            latitude,
            longitude,
            altitude_m,
            fix_quality,
            satellites_used,
            hdop,
        })
    }
}

// ── GSV (Satellites in View) ──────────────────────────────────────────────────

/// A single satellite entry extracted from an NMEA GSV sentence.
#[derive(Debug, Clone)]
pub struct GsvSatellite {
    /// Satellite PRN number.
    pub prn: u8,
    /// Signal-to-noise ratio in dB-Hz, or `None` if the satellite is not
    /// currently being tracked.
    pub snr: Option<u8>,
}

/// Aggregated satellite data from a complete NMEA GSV sequence.
///
/// A single GSV sequence may span multiple sentences (up to 4 satellites per
/// sentence).  This struct is emitted once the final sentence in the sequence
/// has been received, so `satellites` contains the full set.
#[derive(Debug, Clone)]
pub struct GsvData {
    /// All satellites reported in this sequence (from all constellations that
    /// emitted a GSV sentence at this epoch).
    pub satellites: Vec<GsvSatellite>,
}

impl GsvData {
    /// Compute the average SNR over all satellites that have a valid reading.
    ///
    /// Satellites without an SNR value (e.g. acquired but not tracked) are
    /// excluded from the average.  Returns `0` if no satellite has an SNR.
    pub fn avg_snr(&self) -> u8 {
        let mut sum: u32 = 0;
        let mut count: u32 = 0;
        for sat in &self.satellites {
            if let Some(snr) = sat.snr {
                sum += snr as u32;
                count += 1;
            }
        }
        if count > 0 { (sum / count) as u8 } else { 0 }
    }
}
