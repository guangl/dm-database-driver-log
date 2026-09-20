use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn temp_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "dm-driver-log-integration-{name}-{nonce}-{}-{sequence}.log",
        std::process::id()
    ))
}

struct FutureDriverEvent {
    method: String,
    used_time_ms: Option<f64>,
    exec_id: Option<i64>,
    category: &'static str,
}

impl dm_database_driver_log::advanced::LogRecord for FutureDriverEvent {
    fn method(&self) -> &str {
        &self.method
    }

    fn category(&self) -> &str {
        self.category
    }

    fn used_time_ms(&self) -> Option<f64> {
        self.used_time_ms
    }

    fn exec_id(&self) -> Option<i64> {
        self.exec_id
    }
}

struct FutureDriverFormat;

impl dm_database_driver_log::advanced::LogFormat for FutureDriverFormat {
    type Event = FutureDriverEvent;

    const FRAMING: dm_database_driver_log::advanced::RecordFraming =
        dm_database_driver_log::advanced::RecordFraming::Line;

    fn is_record_start(_line: &str) -> bool {
        true
    }

    fn parse_record(
        record: &str,
        _line_number: u64,
    ) -> Result<Self::Event, dm_database_driver_log::ParseError> {
        let (method, used_time_ms, exec_id) = match record {
            "SELECT" => ("SELECT", Some(3.0), Some(100)),
            "UPDATE" => ("UPDATE", Some(0.5), Some(101)),
            _ => (record, None, None),
        };
        Ok(FutureDriverEvent {
            method: method.to_owned(),
            used_time_ms,
            exec_id,
            category: "future-driver",
        })
    }
}

#[test]
fn generic_engine_accepts_a_future_driver_adapter() {
    let path = temp_path("future-driver");
    std::fs::write(&path, "SELECT\nUPDATE\nIGNORED\n").unwrap();

    let parser =
        dm_database_driver_log::advanced::LogParserBuilder::<FutureDriverFormat>::new(&path)
            .build()
            .unwrap();
    assert_eq!(parser.path(), path.as_path());
    let selected: Vec<_> = parser
        .iter()
        .unwrap()
        .filter_by_method("SELECT")
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].exec_id, Some(100));

    let slow: Vec<_> = parser
        .iter()
        .unwrap()
        .filter_by_used_time(1.0)
        .filter_map(Result::ok)
        .filter(|event| event.category == "future-driver")
        .collect();
    assert_eq!(slow.len(), 1);
    assert_eq!(slow[0].method, "SELECT");

    let categories: Vec<_> = parser
        .iter()
        .unwrap()
        .filter_by_category("future-driver")
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(categories.len(), 3);

    let executions: Vec<_> = parser
        .iter()
        .unwrap()
        .filter_by_exec_id(101)
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(executions.len(), 1);
    assert_eq!(parser.iter().unwrap().skip_errors().count(), 3);

    std::fs::remove_file(path).unwrap();
}

#[cfg(feature = "jdbc")]
mod jdbc {
    use super::temp_path;
    use dm_database_driver_log::{JdbcEvent, LogEvent, LogParserBuilder};

    fn jdbc(event: &LogEvent) -> &JdbcEvent {
        event.as_jdbc().expect("expected JDBC event")
    }

    const EXEC_LINE: &str = "[INFO  - 2026-09-16 17:45:19.763] tid:119 - [worker] { conn-3, pstmt-854 } executeQuery(): rs-2216; [USED TIME]: 8.5ms; [EXEC_ID]: 19010657;";
    const BIND_LINE: &str = "[INFO  - 2026-09-16 17:45:19.764] tid:119 - [worker] { conn-3, pstmt-854 } setString(Integer, String); [PARAMS]: 1, \"A06093910597367000594686\"; [USED TIME]: 0.5ms;";
    const GET_LINE: &str = "[INFO  - 2026-09-16 17:45:19.765] tid:119 - [worker] { conn-3, pstmt-854, rs-2216 } getString(String): \"value\"; [PARAMS]: \"name\"; [USED TIME]: 2.5ms;";

    #[test]
    fn runs_jdbc_file_from_input_to_filtered_results() {
        let path = temp_path("jdbc");
        std::fs::write(
            &path,
            format!("\r\n{EXEC_LINE}\r\n{BIND_LINE}\r\n{GET_LINE}\r\nnot a jdbc record"),
        )
        .unwrap();

        let parser = LogParserBuilder::new(&path).build().unwrap();
        assert_eq!(parser.format(), dm_database_driver_log::LogFormatKind::Jdbc);
        let results: Vec<_> = parser.iter().unwrap().collect();
        assert_eq!(results.len(), 4);
        assert_eq!(results[0].as_ref().unwrap().line_number(), 2);
        assert_eq!(results[1].as_ref().unwrap().line_number(), 3);
        assert_eq!(results[2].as_ref().unwrap().line_number(), 4);
        assert_eq!(results[3].as_ref().unwrap_err().line_number(), Some(5));

        let execute: Vec<_> = parser
            .iter()
            .unwrap()
            .filter_by_method("executeQuery")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(execute.len(), 1);
        assert_eq!(jdbc(&execute[0]).result_set_id, Some(2216));
        assert_eq!(execute[0].exec_id(), Some(19010657));

        let binds: Vec<_> = parser
            .iter()
            .unwrap()
            .filter_by_category("bind")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(binds.len(), 1);
        assert_eq!(jdbc(&binds[0]).param_index, Some(1));

        let slow: Vec<_> = parser
            .iter()
            .unwrap()
            .filter_by_used_time(2.0)
            .filter_map(Result::ok)
            .collect();
        assert_eq!(slow.len(), 2);

        let executions: Vec<_> = parser
            .iter()
            .unwrap()
            .filter_by_exec_id(19010657)
            .filter_map(Result::ok)
            .collect();
        assert_eq!(executions.len(), 1);

        let (count, total_ms) = parser.iter().unwrap().filter_map(Result::ok).fold(
            (0, 0.0),
            |(count, total_ms), event| {
                (
                    count + 1,
                    total_ms + event.used_time_ms().unwrap_or_default(),
                )
            },
        );
        assert_eq!(count, 3);
        assert_eq!(total_ms, 11.5);
        assert_eq!(parser.iter().unwrap().skip_errors().count(), 3);

        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(feature = "dm-provider")]
mod dm_provider {
    use super::temp_path;
    use dm_database_driver_log::{DmProviderEvent, LogEvent, LogParserBuilder};

    fn provider(event: &LogEvent) -> &DmProviderEvent {
        event.as_dm_provider().expect("expected DM Provider event")
    }

    const ACCESS_LINE: &str = "[INFO  - 2026-09-12 08:37:44.696] tid:34 (IsBackground-True) { B@16900fb } access Cmd:4(); [USED TIME]: 0ns;";
    const SET_COMMAND_FIRST_LINE: &str = "[INFO  - 2026-09-12 08:55:30.191] tid:68 (IsBackground-True) { conn-2095 (sessId:281421579449976), command-4579 } setCommandText(String); [PARAMS]: \"SELECT * FROM T";
    const SET_COMMAND_SECOND_LINE: &str = "FROM T WHERE ID = 1\"; [USED TIME]: 0ns;";
    const EXECUTE_LINE: &str = "[SQL   - 2026-09-12 08:55:30.193] tid:68 (IsBackground-True) { conn-2095 (sessId:281421579449976), command-4579 } ExecuteDbDataReader(CommandBehavior) [RETURN]: dateReader-2971; [PARAMS]: Default; [SQL]: SELECT * FROM T\nFROM T WHERE ID = 1 [USED TIME]: 2ms; [EXEC_ID]: 908131301;";
    const READ_LINE: &str = "[INFO  - 2026-09-12 08:55:30.194] tid:68 (IsBackground-True) { conn-2095 (sessId:281421579449976), command-4579, dateReader-2971 } Read() [RETURN]: True; [USED TIME]: 500000ns;";
    const INVALID_LINE: &str =
        "[INFO  - 2026-09-12 08:55:30.195] tid:bad (IsBackground-True) { conn-2095 } Close();";

    #[test]
    fn runs_provider_file_from_multiline_input_to_filtered_results() {
        let path = temp_path("provider");
        let content = format!(
            "{ACCESS_LINE}\n{SET_COMMAND_FIRST_LINE}\n{SET_COMMAND_SECOND_LINE}\n{EXECUTE_LINE}\n{READ_LINE}\n{INVALID_LINE}"
        );
        std::fs::write(&path, content).unwrap();

        let parser = LogParserBuilder::new(&path).build().unwrap();
        assert_eq!(
            parser.format(),
            dm_database_driver_log::LogFormatKind::DmProvider
        );
        let results: Vec<_> = parser.iter().unwrap().collect();
        assert_eq!(results.len(), 5);

        let access = results[0].as_ref().unwrap();
        assert_eq!(access.line_number(), 1);
        assert_eq!(provider(access).object_id.as_deref(), Some("16900fb"));
        assert_eq!(provider(access).cmd, Some(4));

        let set_command = results[1].as_ref().unwrap();
        assert_eq!(set_command.line_number(), 2);
        assert!(set_command.raw().contains('\n'));
        assert_eq!(set_command.method(), "setCommandText");
        assert_eq!(provider(set_command).arg_types, "String");
        assert!(
            provider(set_command)
                .params
                .as_deref()
                .unwrap()
                .contains("FROM T")
        );

        let execute = results[2].as_ref().unwrap();
        assert_eq!(execute.line_number(), 4);
        assert_eq!(provider(execute).conn_id, Some(2095));
        assert_eq!(provider(execute).session_id, Some(281421579449976));
        assert_eq!(provider(execute).command_id, Some(4579));
        assert_eq!(provider(execute).result_set_id, Some(2971));
        assert_eq!(provider(execute).sql.as_deref().unwrap().lines().count(), 2);
        assert_eq!(execute.used_time_ms(), Some(2.0));
        assert_eq!(execute.exec_id(), Some(908131301));

        let read = results[3].as_ref().unwrap();
        assert_eq!(read.line_number(), 6);
        assert_eq!(read.category(), "fetch");
        assert_eq!(read.used_time_ms(), Some(0.5));
        assert_eq!(results[4].as_ref().unwrap_err().line_number(), Some(7));

        let sql_records: Vec<_> = parser
            .iter()
            .unwrap()
            .filter_by_category("execute")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(sql_records.len(), 1);
        assert_eq!(sql_records[0].exec_id(), Some(908131301));

        let access_records: Vec<_> = parser
            .iter()
            .unwrap()
            .filter_by_method("access")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(access_records.len(), 1);

        let slow: Vec<_> = parser
            .iter()
            .unwrap()
            .filter_by_used_time(1.0)
            .filter_map(Result::ok)
            .collect();
        assert_eq!(slow.len(), 1);

        let executions: Vec<_> = parser
            .iter()
            .unwrap()
            .filter_by_exec_id(908131301)
            .filter_map(Result::ok)
            .collect();
        assert_eq!(executions.len(), 1);

        let (count, sql_count, total_ms) = parser.iter().unwrap().filter_map(Result::ok).fold(
            (0, 0, 0.0),
            |(count, sql_count, total_ms), event| {
                (
                    count + 1,
                    sql_count
                        + usize::from(
                            event
                                .as_dm_provider()
                                .is_some_and(|provider| provider.sql.is_some()),
                        ),
                    total_ms + event.used_time_ms().unwrap_or_default(),
                )
            },
        );
        assert_eq!(count, 4);
        assert_eq!(sql_count, 1);
        assert_eq!(total_ms, 2.5);
        assert_eq!(parser.iter().unwrap().skip_errors().count(), 4);

        std::fs::remove_file(path).unwrap();
    }
}
