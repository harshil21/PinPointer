use nmea::{self, Nmea};

pub fn build_parser() -> Nmea {
    let parser = Nmea::default();
    parser
}


pub fn parse_nmea_sentence(
    parser: &mut Nmea,
    input: &str,
) -> () {

    let _ = parser.parse(input);
}
