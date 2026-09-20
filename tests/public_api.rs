#![cfg(feature = "jdbc")]

use dm_database_driver_log::{
    FileEncodingHint, parse_bytes, parse_bytes_with_encoding, parse_line,
};

const LINE: &str = "[INFO  - 2026-09-16 17:45:19.763] tid:119 - [worker] { conn-3, pstmt-854, rs-2216 } getString(String): \"value\"; [PARAMS]: \"name\"; [USED TIME]: 0.5ms;";

#[test]
fn owned_single_line_api_exposes_structured_event() {
    let event = parse_line(LINE).unwrap();

    assert_eq!(event.line_number, 0);
    assert_eq!(event.method, "getString");
    assert_eq!(event.column_name.as_deref(), Some("name"));
    assert_eq!(event.result_set_id, Some(2216));
    assert_eq!(event.category, "read_value");
}

#[test]
fn legacy_parser_module_still_points_to_jdbc_format() {
    let event = dm_database_driver_log::parser::parse(LINE).unwrap();

    assert_eq!(event.method, "getString");
    assert_eq!(event.category, "read_value");
}

#[test]
fn byte_api_accepts_a_line_without_utf8_assumptions() {
    let event = parse_bytes(LINE.as_bytes()).unwrap();

    assert_eq!(event.event_time_text, "2026-09-16 17:45:19.763");
    assert_eq!(event.used_time_ms, Some(0.5));
}

#[test]
fn byte_api_accepts_explicit_gb18030() {
    use ::encoding::all::GB18030;
    use ::encoding::{EncoderTrap, Encoding};

    let line = "[INFO - 2026-09-16 17:45:19.763] tid:119 - [线程] { conn-3 } close();";
    let bytes = GB18030.encode(line, EncoderTrap::Strict).unwrap();
    let event = parse_bytes_with_encoding(&bytes, FileEncodingHint::Gb18030).unwrap();

    assert_eq!(event.thread, "线程");
}
