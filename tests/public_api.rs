#![cfg(feature = "jdbc")]

use dm_database_driver_log::{
    FileEncodingHint, LogFormatKind, parse_bytes, parse_bytes_with_encoding, parse_line,
};

const LINE: &str = "[INFO  - 2026-09-16 17:45:19.763] tid:119 - [worker] { conn-3, pstmt-854, rs-2216 } getString(String): \"value\"; [PARAMS]: \"name\"; [USED TIME]: 0.5ms;";

#[test]
fn owned_single_line_api_exposes_structured_event() {
    let event = parse_line(LINE).unwrap();
    let event = event.as_jdbc().unwrap();

    assert_eq!(event.line_number, 0);
    assert_eq!(event.method, "getString");
    assert_eq!(event.column_name.as_deref(), Some("name"));
    assert_eq!(event.result_set_id, Some(2216));
    assert_eq!(event.category, "read_value");
}

#[test]
fn unified_parser_returns_jdbc_events() {
    let event = parse_line(LINE).unwrap();

    assert_eq!(event.method(), "getString");
    assert_eq!(event.category(), "read_value");
}

#[test]
fn unified_event_exposes_common_and_format_specific_api() {
    let event = parse_line(LINE).unwrap();
    assert_eq!(event.format(), LogFormatKind::Jdbc);
    assert_eq!(event.line_number(), 0);
    assert_eq!(event.raw(), LINE);
    assert_eq!(event.method(), "getString");
    assert_eq!(event.category(), "read_value");
    assert_eq!(event.used_time_ms(), Some(0.5));
    assert_eq!(event.exec_id(), None);
    assert!(event.as_jdbc().is_some());

    #[cfg(feature = "dm-provider")]
    {
        assert!(event.as_dm_provider().is_none());
        let provider = parse_line(
            "[INFO - 2026-09-12 08:37:44.696] tid:34 (worker) { conn-3 } access Cmd:4();",
        )
        .unwrap();
        assert_eq!(provider.format(), LogFormatKind::DmProvider);
        assert!(provider.as_jdbc().is_none());
        assert!(provider.as_dm_provider().is_some());
    }
}

#[test]
fn byte_api_accepts_a_line_without_utf8_assumptions() {
    let event = parse_bytes(LINE.as_bytes()).unwrap();
    let event = event.as_jdbc().unwrap();

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

    assert_eq!(event.as_jdbc().unwrap().thread, "线程");
}
