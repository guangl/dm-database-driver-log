#![cfg(feature = "dm-provider")]

use dm_database_driver_log::{
    DmProviderLogParserBuilder, FileEncodingHint, ParseError, dm_provider,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const ACCESS: &str = "[INFO  - 2026-09-12 08:37:44.696] tid:34 (IsBackground-True) { B@16900fb } access Cmd:4();   [USED TIME]: 0ns;";
const SQL: &str = "[SQL   - 2026-09-12 08:55:30.193] tid:68 (IsBackground-True) { conn-2095 (sessId:281421579449976), command-4579 } ExecuteDbDataReader(CommandBehavior) [RETURN]: dateReader-2971;   [PARAMS]: Default; [SQL]: SELECT * FROM T\nWHERE ID = 1 [USED TIME]: 2ms; [EXEC_ID]: 908131301;";
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn temp_path() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "dm-provider-log-{nonce}-{}-{sequence}.log",
        std::process::id()
    ))
}

#[test]
fn parses_access_and_provider_ids() {
    let event = dm_provider::parse_line(ACCESS).unwrap();
    assert_eq!(event.level, "INFO");
    assert_eq!(event.thread, "IsBackground-True");
    assert_eq!(event.object_id.as_deref(), Some("16900fb"));
    assert_eq!(event.method, "access");
    assert_eq!(event.arg_types, "Cmd:4");
    assert_eq!(event.cmd, Some(4));
    assert_eq!(event.used_time_text.as_deref(), Some("0ns"));
    assert_eq!(event.used_time_ms, Some(0.0));
    assert_eq!(event.category, "driver_access");
}

#[test]
fn parses_multiline_sql_and_numeric_fields() {
    let event = dm_provider::parse_line(SQL).unwrap();
    assert_eq!(event.conn_id, Some(2095));
    assert_eq!(event.session_id, Some(281421579449976));
    assert_eq!(event.command_id, Some(4579));
    assert_eq!(event.result_set_id, Some(2971));
    assert_eq!(event.params.as_deref(), Some("Default"));
    assert_eq!(event.sql.as_deref(), Some("SELECT * FROM T\nWHERE ID = 1"));
    assert_eq!(event.used_time_ms, Some(2.0));
    assert_eq!(event.exec_id, Some(908131301));
    assert_eq!(event.category, "execute");
}

#[test]
fn parses_messages_and_other_methods() {
    let message = dm_provider::parse_line(
        "[INFO - 2026-09-12 08:55:30.190] tid:68 (IsBackground-True) try connect loop 0",
    )
    .unwrap();
    assert_eq!(message.method, "try connect loop 0");
    assert_eq!(message.category, "message");

    let bind = dm_provider::parse_line(
        "[INFO - 2026-09-12 08:55:30.191] tid:68 (IsBackground-True) { parameter-7 } Add(DmParameter) [RETURN]: 0; [PARAMS]: x; [USED TIME]: 500us;",
    )
    .unwrap();
    assert_eq!(bind.method, "Add");
    assert_eq!(bind.arg_types, "DmParameter");
    assert_eq!(bind.used_time_ms, Some(0.5));
    assert_eq!(bind.category, "bind");
}

#[test]
fn iterator_groups_sql_continuations_and_supports_filters() {
    let path = temp_path();
    std::fs::write(
        &path,
        format!("{ACCESS}\n{SQL}\n[INFO - 2026-09-12 08:55:30.194] tid:bad\n"),
    )
    .unwrap();
    let parser = DmProviderLogParserBuilder::new(&path).build().unwrap();
    let events: Vec<_> = parser.iter().unwrap().collect();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].as_ref().unwrap().line_number, 1);
    assert_eq!(events[1].as_ref().unwrap().line_number, 2);
    assert_eq!(events[1].as_ref().unwrap().raw, SQL);
    assert_eq!(events[2].as_ref().unwrap_err().line_number(), Some(4));

    let filtered: Vec<_> = parser
        .iter()
        .unwrap()
        .filter_by_exec_id(908131301)
        .filter_map(Result::ok)
        .collect();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].method, "ExecuteDbDataReader");

    let filtered: Vec<_> = parser
        .iter()
        .unwrap()
        .filter_by_category("driver_access")
        .filter_map(Result::ok)
        .collect();
    assert_eq!(filtered.len(), 1);

    let filtered: Vec<_> = parser
        .iter()
        .unwrap()
        .filter_by_method("ExecuteDbDataReader")
        .filter_map(Result::ok)
        .collect();
    assert_eq!(filtered.len(), 1);

    let filtered: Vec<_> = parser
        .iter()
        .unwrap()
        .filter_by_used_time(1.0)
        .filter_map(Result::ok)
        .collect();
    assert_eq!(filtered.len(), 1);

    let parser = DmProviderLogParserBuilder::new(&path)
        .encoding_hint(FileEncodingHint::Utf8)
        .build()
        .unwrap();
    assert_eq!(parser.iter().unwrap().skip_errors().count(), 2);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn reports_io_and_malformed_records() {
    let path = temp_path();
    std::fs::write(&path, ACCESS).unwrap();
    let parser = DmProviderLogParserBuilder::new(&path).build().unwrap();
    assert_eq!(parser.path(), path.as_path());
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(parser.iter(), Err(ParseError::IoError(_))));

    let error = DmProviderLogParserBuilder::new(&path).build().unwrap_err();
    assert!(matches!(error, ParseError::IoError(_)));

    let error = dm_provider::parse_line("[INFO").unwrap_err();
    assert_eq!(error.line_number(), None);
    assert!(error.to_string().contains("unterminated level header"));
}

fn with_duration(duration: &str) -> String {
    format!(
        "[INFO - 2026-09-12 08:37:44.696] tid:34 (worker) {{ conn-3 }} access Cmd:4(); [USED TIME]: {duration};"
    )
}

#[test]
fn parses_duration_variants_bytes_and_timestamps() {
    assert_eq!(
        dm_provider::parse_line(&with_duration("1ms"))
            .unwrap()
            .used_time_ms,
        Some(1.0)
    );
    assert_eq!(
        dm_provider::parse_line(&with_duration("2s"))
            .unwrap()
            .used_time_ms,
        Some(2000.0)
    );
    assert_eq!(
        dm_provider::parse_line(&with_duration("3ns"))
            .unwrap()
            .used_time_ms,
        Some(0.000003)
    );
    assert_eq!(
        dm_provider::parse_line(&with_duration("500us"))
            .unwrap()
            .used_time_ms,
        Some(0.5)
    );
    assert_eq!(
        dm_provider::parse_line(&with_duration("bad"))
            .unwrap()
            .used_time_ms,
        None
    );
    assert_eq!(
        dm_provider::parse_line(&with_duration("1m"))
            .unwrap()
            .used_time_ms,
        None
    );

    let event = dm_provider::parse_bytes(ACCESS.as_bytes()).unwrap();
    assert_eq!(event.cmd, Some(4));
    assert_eq!(event.event_time_ms, Some(1_789_202_264_696));
}
