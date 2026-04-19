// src/display.rs
use crossterm::{
    cursor::MoveUp,
    queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};
use std::io::Write;

use nmea::{sentences::FixType, Nmea};

pub struct Display {
    prev_lines: u16,
}

// Build display items
enum DisplayItem {
    Header(String, Color),
    Divider(String, Color),
    Data(String, String, Option<String>),
}


impl Display {
    pub fn new() -> Self {
        Self { prev_lines: 0 }
    }

    pub fn update<W: Write>(
        &mut self,
        stdout: &mut W,
        parser: &Nmea,
        avg_snr_value: u32,
    ) -> std::io::Result<()> {
        let items = vec![
            DisplayItem::Header("=== GPS DATA ===".to_string(), Color::Yellow),
            DisplayItem::Data(
                "Timestamp".to_string(),
                format!("{:?}", parser.fix_time.unwrap_or_default()),
                None,
            ),
            DisplayItem::Data(
                "Latitude".to_string(),
                format!("{:?}", parser.latitude.unwrap_or_default()),
                None,
            ),
            DisplayItem::Data(
                "Longitude".to_string(),
                format!("{:?}", parser.longitude.unwrap_or_default()),
                None,
            ),
            DisplayItem::Data(
                "Altitude".to_string(),
                format!("{:?}", parser.altitude.unwrap_or_default()),
                Some("m".to_string()),
            ),
            DisplayItem::Data(
                "Fix Type".to_string(),
                format!("{:?}", parser.fix_type.unwrap_or_else(|| FixType::Simulation)),
                None,
            ),
            DisplayItem::Data(
                "Speed".to_string(),
                format!("{:?}", parser.speed_over_ground.unwrap_or_default()),
                Some("km/h".to_string()),
            ),
            DisplayItem::Data(
                "Number of Satellites".to_string(),
                format!("{:?}", parser.num_of_fix_satellites.unwrap_or_default()),
                None,
            ),
            DisplayItem::Data(
                "HDOP".to_string(),
                format!("{:?}", parser.hdop.unwrap_or_default()),
                None,
            ),
            DisplayItem::Data(
                "VDOP".to_string(),
                format!("{:?}", parser.vdop.unwrap_or_default()),
                None,
            ),
            DisplayItem::Data(
                "PDOP".to_string(),
                format!("{:?}", parser.pdop.unwrap_or_default()),
                None,
            ),
            DisplayItem::Divider("=================".to_string(), Color::Yellow),
            DisplayItem::Data(
                "Avg SNR".to_string(),
                format!("{:?}", avg_snr_value),
                Some("db-Hz".to_string()),
            ),
        ];

        // Calculate max label length for padding
        let max_len = items
            .iter()
            .filter_map(|item| {
                if let DisplayItem::Data(label, _, _) = item {
                    Some(label.len())
                } else {
                    None
                }
            })
            .max()
            .unwrap_or(0);

        // Calculate line count (each item contributes one line)
        let line_count = items.len() as u16;

        // Move up and clear if not the first iteration
        if self.prev_lines > 0 {
            queue!(stdout, MoveUp(self.prev_lines), Clear(ClearType::FromCursorDown))?;
        }

        // Queue the display commands
        for item in items {
            match item {
                DisplayItem::Header(s, c) | DisplayItem::Divider(s, c) => {
                    queue!(
                        stdout,
                        SetForegroundColor(c),
                        Print(s),
                        ResetColor,
                        Print("\n".to_string())
                    )?;
                }
                DisplayItem::Data(label, value, opt_unit) => {
                    let padded = format!("{:<width$}: ", label, width = max_len);
                    queue!(stdout, Print(padded))?;
                    queue!(stdout, SetForegroundColor(Color::Green), Print(value), ResetColor)?;
                    if let Some(unit) = opt_unit {
                        queue!(stdout, Print(" ".to_string()), SetForegroundColor(Color::Red), Print(unit), ResetColor)?;
                    }
                    queue!(stdout, Print("\n".to_string()))?;
                }
            }
        }

        stdout.flush()?;

        // Update previous line count
        self.prev_lines = line_count;

        Ok(())
    }
}