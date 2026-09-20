#![cfg(feature = "jdbc")]

use dm_database_driver_log::{JdbcEvent, LogParserBuilder, ParseError, parse_line};

const SQL_LINE: &str = "[SQL   - 2026-09-16 17:45:19.754] tid:119 - [http-nio-10023-exec-2] { conn-3, pstmt-854, handle-3, sessionID-0x6c0504a0 } prepareStatement(String): pstmt-854, handle-3, sessionID-0x6c0504a0;  [PARAMS]: \"select * from t where a=? and b='x;y'\"; [USED TIME]: 0.600154ms;";
const GET_LINE: &str = "[INFO  - 2026-09-16 17:45:19.763] tid:119 - [http-nio-10023-exec-2] { conn-3, pstmt-854, rs-2216 } getString(String): \"A06103045250071000144881\";  [PARAMS]: \"id_pirec1_119_\"; [USED TIME]: 0.004311ms;";
const BIND_LINE: &str = "[INFO  - 2026-09-16 17:45:19.754] tid:119 - [http-nio-10023-exec-2] { conn-3, pstmt-854 } setString(Integer, String);  [PARAMS]: 1, \"A06093910597367000594686\"; [USED TIME]: 9.9E-4ms;";
const EXEC_LINE: &str = "[INFO  - 2026-09-16 17:45:19.763] tid:119 - [http-nio-10023-exec-2] { conn-3, pstmt-854 } executeQuery(): rs-2216;  [USED TIME]: 8.97386ms; [EXEC_ID]: 19010657;";
fn parse(line: &str) -> Result<JdbcEvent, ParseError> {
    parse_line(line).map(|event| event.as_jdbc().cloned().expect("expected JDBC event"))
}

const ACCESS_LINE: &str = "[DEBUG - 2026-09-16 17:45:19.754] tid:119 - [http-nio-10023-exec-2] { conn-3 } access();  CMD_EXECUTE2";

#[test]
fn parses_prepare_statement_line() {
    let event = parse(SQL_LINE).unwrap();
    assert_eq!(event.level, "SQL");
    assert_eq!(event.thread, "http-nio-10023-exec-2");
    assert_eq!(event.conn_id, Some(3));
    assert_eq!(event.pstmt_id, Some(854));
    assert_eq!(event.handle_id, Some(3));
    assert_eq!(event.session_id_hex.as_deref(), Some("0x6c0504a0"));
    assert_eq!(event.method, "prepareStatement");
    assert_eq!(event.category, "prepare");
    assert_eq!(event.used_time_ms, Some(0.600154));
    assert!(event.params.unwrap().ends_with("b='x;y'\""));
    assert!(event.column_name.is_none());

    assert_eq!(event.line_number, 0);
    assert_eq!(event.raw, SQL_LINE);
}

#[test]
fn parses_column_read_line() {
    let event = parse(GET_LINE).unwrap();
    assert_eq!(event.column_name.as_deref(), Some("id_pirec1_119_"));
    assert_eq!(
        event.return_value.as_deref(),
        Some("\"A06103045250071000144881\"")
    );
    assert_eq!(event.result_set_id, Some(2216));
    assert_eq!(event.category, "read_value");
    assert_eq!(event.event_time_ms, Some(1_789_580_719_763));
}

#[test]
fn parses_bind_execute_and_debug_access_lines() {
    let bind = parse(BIND_LINE).unwrap();
    assert_eq!(bind.param_index, Some(1));
    assert_eq!(
        bind.param_value.as_deref(),
        Some("\"A06093910597367000594686\"")
    );
    assert_eq!(bind.used_time_ms, Some(0.00099));
    assert_eq!(bind.category, "bind");

    let execute = parse(EXEC_LINE).unwrap();
    assert_eq!(execute.result_set_id, Some(2216));
    assert_eq!(execute.rs_id, None);
    assert_eq!(execute.exec_id, Some(19010657));
    assert_eq!(execute.category, "execute");

    let access = parse(ACCESS_LINE).unwrap();
    assert_eq!(access.level, "DEBUG");
    assert_eq!(access.cmd.as_deref(), Some("CMD_EXECUTE2"));
    assert_eq!(access.used_time_ms, None);
    assert_eq!(access.return_value, None);
    assert_eq!(access.category, "driver_access");
}

#[test]
fn parses_epoch_and_rejects_foreign_lines() {
    let event = parse(GET_LINE).unwrap();
    assert_eq!(event.event_time_ms, Some(1_789_580_719_763));

    let invalid_timestamp = "[INFO - 2026-09-16 17:45:19.xxx] tid:1 - [worker] { conn-1 } close();";
    assert_eq!(parse(invalid_timestamp).unwrap().event_time_ms, None);
    assert!(parse("2026-09-16 something else").is_err());
    assert!(parse("").is_err());
}

#[test]
fn rejects_each_malformed_header_section() {
    const HEADER: &str = "[INFO - 2026-09-16 17:45:19.763]";
    let error_for = |suffix: &str| parse(&format!("{HEADER}{suffix}")).unwrap_err();

    assert!(
        parse("[INFO")
            .unwrap_err()
            .to_string()
            .contains("unterminated level header")
    );
    assert!(error_for("").to_string().contains("expected 'tid:'"));
    assert!(
        parse("[INFO 2026-09-16 17:45:19.763] tid:1")
            .unwrap_err()
            .to_string()
            .contains("missing ' - '")
    );
    assert!(error_for(" tid:x").to_string().contains("bad tid"));
    assert!(
        error_for(" tid:1 nope")
            .to_string()
            .contains("expected '-'")
    );
    assert!(
        error_for(" tid:1 - worker")
            .to_string()
            .contains("expected '['")
    );
    assert!(
        error_for(" tid:1 - [worker")
            .to_string()
            .contains("unterminated thread")
    );
    assert!(
        error_for(" tid:1 - [worker] method")
            .to_string()
            .contains("expected '{'")
    );
    assert!(
        error_for(" tid:1 - [worker] { conn-1")
            .to_string()
            .contains("unterminated id block")
    );
    assert!(
        error_for(" tid:1 - [worker] {} method")
            .to_string()
            .contains("missing '('")
    );
    assert!(
        error_for(" tid:1 - [worker] {} method(")
            .to_string()
            .contains("unterminated method args")
    );
}

#[test]
fn parses_empty_fields_escaped_quotes_and_tail_markers() {
    let line = "[INFO - 2026-09-16 17:45:19.763] tid:119 - [worker] { handle-7 } close(): ; [PARAMS]: ; [USED TIME]: invalidms; [EXEC_ID]: nope;";
    let event = parse(line).unwrap();
    assert_eq!(event.handle_id, Some(7));
    assert!(event.return_value.is_none());
    assert!(event.params.is_none());
    assert_eq!(event.used_time_text.as_deref(), Some("invalid"));
    assert_eq!(event.used_time_ms, None);
    assert_eq!(event.exec_id, None);

    let escaped = r#"[INFO - 2026-09-16 17:45:19.763] tid:119 - [worker] { conn-3 } query(): value; [PARAMS]: "a\"; [USED TIME]: 99ms"; [USED TIME]: 0.5ms;"#;
    let event = parse(escaped).unwrap();
    assert_eq!(event.params.as_deref(), Some(r#""a\"; [USED TIME]: 99ms""#));
    assert_eq!(event.used_time_ms, Some(0.5));

    let quoted = "[SQL   - 2026-09-16 17:45:19.754] tid:119 - [worker] { conn-3 } prepareStatement(String): pstmt-854; [PARAMS]: \"select '; [USED TIME]: 999ms; [EXEC_ID]: 1;'\"; [USED TIME]: 0.600154ms; [EXEC_ID]: 19010657;";
    let event = parse(quoted).unwrap();
    assert_eq!(
        event.params.as_deref(),
        Some("\"select '; [USED TIME]: 999ms; [EXEC_ID]: 1;'\"")
    );
    assert_eq!(event.used_time_ms, Some(0.600154));
    assert_eq!(event.exec_id, Some(19010657));
}

#[test]
fn public_categories_cover_common_jdbc_methods() {
    let cases = [
        ("", "unparsed"),
        ("prepareStatement", "prepare"),
        ("setString", "bind"),
        ("executeQuery", "execute"),
        ("next", "fetch_next"),
        ("wasNull", "null_check"),
        ("clearParameters", "clear"),
        ("close", "close"),
        ("access", "driver_access"),
        ("getString", "read_value"),
        ("getMetaData", "read_meta"),
        ("commit", "other"),
    ];

    for (method, category) in cases {
        let line =
            format!("[INFO - 2026-09-16 17:45:19.763] tid:1 - [worker] {{ conn-1 }} {method}():;");
        assert_eq!(parse(&line).unwrap().category, category, "{method}");
    }
}

#[test]
fn parse_errors_expose_public_context_and_accessors() {
    let error = parse("not a driver line").unwrap_err();
    assert_eq!(error.line_number(), None);
    assert!(error.offset().is_some());
    assert!(error.to_string().contains("not a driver line"));
    assert!(
        ParseError::IoError("disk".to_owned())
            .to_string()
            .contains("disk")
    );
    assert_eq!(ParseError::IoError("disk".to_owned()).offset(), None);
}

#[test]
fn parser_errors_keep_file_context() {
    let path = std::env::temp_dir().join(format!(
        "dm-jdbc-missing-{}-{}.log",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let error = LogParserBuilder::new(&path).build().unwrap_err();
    assert!(matches!(error, ParseError::IoError(_)));
}
