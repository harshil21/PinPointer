use crossterm::execute;
use std::io::{stdout};

mod serial;
mod gps;
mod display;

fn main() -> std::io::Result<()> {
    let mut port = serial::open_port();
    let mut parser = gps::parser::build_parser();
    let mut stdout = stdout();
    let mut display = display::Display::new();

    // Initialize terminal (enable raw mode if needed for advanced input, but not required here)
    execute!(stdout, crossterm::cursor::SetCursorStyle::BlinkingBlock)?;

    loop {
        let serial_data = port.as_mut().unwrap().read_sentences();

        if serial_data.is_err() {
            continue;
        }
        let sentences = serial_data.unwrap();

        for sentence in sentences {
            // println!("NMEA Sentence: {}", sentence);
            gps::parser::parse_nmea_sentence(&mut parser, &sentence);
        }


        // Get satellites info (like SNR):
        let sats = parser.satellites();
        let mut avg_snr = 0u32;
        let mut count = 0u32;
        for sat in &sats {
            if let Some(snr) = sat.snr() {
                avg_snr += snr as u32;
                count += 1;
            }
        }
        let avg_snr_value = if count > 0 { avg_snr / count } else { 0 };

        display.update(&mut stdout, &parser, avg_snr_value)?;
    }
}