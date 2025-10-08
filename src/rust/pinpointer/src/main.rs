mod serial;
mod gps;

fn main() {
    let mut port = serial::open_port();
    let mut parser = gps::parser::build_parser();


    loop {
        let serial_data = port.as_mut().unwrap().read_sentences();

        if serial_data.is_err() {
            continue;
        }
        let sentences = serial_data.unwrap();

        for sentence in sentences {
            println!("NMEA Sentence: {}", sentence);
            let _ = gps::parser::parse_nmea_sentence(&mut parser, &sentence);
        }
        // let _ = gps::parser::parse_nmea_sentence(&mut parser, &);

        // TODO: make a display function like in Airbrakes
        // println!("{:?}", parser);
    }
}
