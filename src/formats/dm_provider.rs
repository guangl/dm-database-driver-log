//! DM Provider 日志格式解析器。
//!
//! 该格式与 JDBC 日志不同：线程信息使用圆括号，ID 使用大括号，耗时可能是
//! `ns` 或 `ms`，并且 SQL 常常跨越多行。本模块通过 `dm-provider` feature
//! 启用，避免把两套格式的状态机混在同一个迭代器里。

use crate::core::{LogFormat, RecordFraming};
use crate::error::ParseError;

const MARK_RETURN: &str = "[RETURN]:";
const MARK_PARAMS: &str = "[PARAMS]:";
const MARK_SQL: &str = "[SQL]:";
const MARK_USED_TIME: &str = "[USED TIME]:";
const MARK_EXEC_ID: &str = "[EXEC_ID]:";

/// 一条 DM Provider 日志记录。
#[derive(Debug, Clone, PartialEq)]
pub struct DmProviderEvent {
    pub line_number: u64,
    pub raw: String,
    pub level: String,
    pub event_time_text: String,
    pub event_time_ms: Option<i64>,
    pub tid: Option<i32>,
    pub thread: String,
    /// 大括号内的原始对象标识，保留 `conn-... (sessId:...)` 等完整信息。
    pub ids: String,
    pub object_id: Option<String>,
    pub conn_id: Option<i32>,
    pub session_id: Option<i64>,
    pub command_id: Option<i32>,
    pub result_set_id: Option<i32>,
    pub parameter_collection_id: Option<i32>,
    pub method: String,
    pub arg_types: String,
    pub return_value: Option<String>,
    pub params: Option<String>,
    pub sql: Option<String>,
    pub used_time_text: Option<String>,
    /// 统一换算成毫秒；例如 `500000ns` 是 `Some(0.5)`。
    pub used_time_ms: Option<f64>,
    pub exec_id: Option<i64>,
    /// `access Cmd:4()` 中的数字命令编号。
    pub cmd: Option<i32>,
    pub category: &'static str,
}

/// DM Provider 日志格式适配器。
#[derive(Copy, Clone, Debug, Default)]
pub struct DmProviderFormat;

impl LogFormat for DmProviderFormat {
    type Event = DmProviderEvent;

    const FRAMING: RecordFraming = RecordFraming::HeaderDelimited;

    fn is_record_start(line: &str) -> bool {
        is_record_start(line)
    }

    fn parse_record(record: &str, line_number: u64) -> Result<Self::Event, ParseError> {
        parse_line(record).map(|mut event| {
            event.line_number = line_number;
            event.raw = record.to_owned();
            event
        })
    }
}

/// 解析一条 Provider 记录。记录可以包含 SQL 的换行内容。
pub fn parse_line(record: &str) -> Result<DmProviderEvent, ParseError> {
    parse_inner(record).map_err(|error| error.with_context(record, 0))
}

fn parse_inner(record: &str) -> Result<DmProviderEvent, ParseError> {
    let bytes = record.as_bytes();
    if bytes.is_empty() {
        return Err(ParseError::invalid(0, "empty record"));
    }
    if bytes[0] != b'[' {
        return Err(ParseError::invalid(0, "record does not start with '['"));
    }

    let header_end =
        find(bytes, b"]", 1).ok_or(ParseError::invalid(1, "unterminated level header"))?;
    let header = &bytes[1..header_end];
    let separator =
        find(header, b" - ", 0).ok_or(ParseError::invalid(1, "missing ' - ' in header"))?;
    let level = trim(&header[..separator]);
    let event_time_text = trim(&header[separator + 3..]);

    let mut cursor = skip_whitespace(bytes, header_end + 1);
    expect(bytes, cursor, b"tid:")?;
    cursor += 4;
    let (tid, next) = take_i64(bytes, cursor).ok_or(ParseError::invalid(cursor, "bad tid"))?;
    cursor = skip_whitespace(bytes, next);
    expect(bytes, cursor, b"(")?;
    let thread_end =
        find(bytes, b")", cursor + 1).ok_or(ParseError::invalid(cursor, "unterminated thread"))?;
    let thread = trim(&bytes[cursor + 1..thread_end]);
    cursor = skip_whitespace(bytes, thread_end + 1);

    let (ids, body_start) = if bytes.get(cursor) == Some(&b'{') {
        let ids_end = find(bytes, b"}", cursor + 1)
            .ok_or(ParseError::invalid(cursor, "unterminated id block"))?;
        (
            trim(&bytes[cursor + 1..ids_end]),
            skip_whitespace(bytes, ids_end + 1),
        )
    } else {
        ("", cursor)
    };
    let body = trim_start(&bytes[body_start..]);
    if body.is_empty() {
        return Err(ParseError::invalid(body_start, "missing provider body"));
    }

    let marker_positions = marker_positions(body);
    let method_part_end = marker_positions
        .values()
        .copied()
        .min()
        .unwrap_or(body.len());
    let method_part = trim(&body.as_bytes()[..method_part_end]);
    let (method, arg_types) = parse_method(method_part);

    let mut event = DmProviderEvent {
        line_number: 0,
        raw: String::new(),
        level: level.to_owned(),
        event_time_text: event_time_text.to_owned(),
        event_time_ms: parser_epoch_millis(event_time_text),
        tid: i32::try_from(tid).ok(),
        thread: thread.to_owned(),
        ids: ids.to_owned(),
        object_id: None,
        conn_id: None,
        session_id: None,
        command_id: None,
        result_set_id: None,
        parameter_collection_id: None,
        method: method.to_owned(),
        arg_types: arg_types.to_owned(),
        return_value: value_for(body, &marker_positions, MARK_RETURN),
        params: value_for(body, &marker_positions, MARK_PARAMS),
        sql: value_for(body, &marker_positions, MARK_SQL),
        used_time_text: None,
        used_time_ms: None,
        exec_id: value_for(body, &marker_positions, MARK_EXEC_ID)
            .and_then(|value| value.parse::<i64>().ok()),
        cmd: None,
        category: category_of(method),
    };

    if let Some(used_time) = value_for(body, &marker_positions, MARK_USED_TIME) {
        event.used_time_ms = parse_duration_ms(&used_time);
        event.used_time_text = Some(used_time);
    }
    parse_ids(ids, &mut event);
    if let Some(return_value) = event.return_value.as_deref()
        && let Some(value) = digits_after(return_value, "dateReader-")
    {
        event.result_set_id = i32::try_from(value).ok();
    }
    if method == "access"
        && let Some(command) = arg_types.strip_prefix("Cmd:")
    {
        event.cmd = command.trim().parse::<i32>().ok();
    }

    Ok(event)
}

fn parse_method(method_part: &str) -> (&str, &str) {
    let Some(open) = method_part.find('(') else {
        return (method_part.trim(), "");
    };
    let close = method_part[open + 1..]
        .find(')')
        .map(|offset| open + 1 + offset)
        .unwrap_or(method_part.len());
    let prefix = method_part[..open].trim();
    let arguments = method_part[open + 1..close].trim();
    if let Some(split) = prefix.find(char::is_whitespace) {
        let method = prefix[..split].trim();
        let prefix_arguments = prefix[split..].trim();
        if arguments.is_empty() {
            (method, prefix_arguments)
        } else {
            (method, arguments)
        }
    } else {
        (prefix, arguments)
    }
}

fn parse_ids(ids: &str, event: &mut DmProviderEvent) {
    for token in ids.split(',') {
        let token = token.trim();
        if let Some(value) = digits_after(token, "conn-") {
            event.conn_id = i32::try_from(value).ok();
        } else if let Some(value) = digits_after(token, "command-") {
            event.command_id = i32::try_from(value).ok();
        } else if let Some(value) = digits_after(token, "dateReader-") {
            event.result_set_id = i32::try_from(value).ok();
        } else if let Some(value) = digits_after(token, "parameterCollection-") {
            event.parameter_collection_id = i32::try_from(value).ok();
        } else if let Some(value) = token.strip_prefix("B@")
            && !value.is_empty()
        {
            event.object_id = Some(value.to_owned());
        }
        if let Some(start) = token.find("sessId:") {
            let value = &token[start + "sessId:".len()..];
            event.session_id = take_decimal(value);
        }
    }
}

fn marker_positions(body: &str) -> std::collections::BTreeMap<&'static str, usize> {
    [
        MARK_RETURN,
        MARK_PARAMS,
        MARK_SQL,
        MARK_USED_TIME,
        MARK_EXEC_ID,
    ]
    .into_iter()
    .filter_map(|marker| {
        find_marker(body.as_bytes(), marker.as_bytes(), 0).map(|pos| (marker, pos))
    })
    .collect()
}

fn value_for(
    body: &str,
    positions: &std::collections::BTreeMap<&'static str, usize>,
    marker: &'static str,
) -> Option<String> {
    let start = positions.get(marker).copied()? + marker.len();
    let end = positions
        .values()
        .copied()
        .filter(|position| *position > start)
        .min()
        .unwrap_or(body.len());
    let value = trim_separators(&body[start..end]);
    (!value.is_empty()).then(|| value.to_owned())
}

fn parse_duration_ms(value: &str) -> Option<f64> {
    let value = value.trim();
    let unit_start = value
        .find(|character: char| character.is_ascii_alphabetic())
        .unwrap_or(value.len());
    let number = value[..unit_start].trim().parse::<f64>().ok()?;
    match value[unit_start..].trim() {
        "ns" => Some(number / 1_000_000.0),
        "us" => Some(number / 1_000.0),
        "ms" => Some(number),
        "s" => Some(number * 1_000.0),
        _ => None,
    }
}

fn category_of(method: &str) -> &'static str {
    if method == "access" {
        "driver_access"
    } else if method.starts_with("Execute") {
        "execute"
    } else if method.starts_with("Read") {
        "fetch"
    } else if method.starts_with("get") {
        "read_value"
    } else if method.starts_with("set") || method == "Add" {
        "bind"
    } else if method == "Close" || method == "Dispose" {
        "close"
    } else if method.is_empty() {
        "unparsed"
    } else if !method.contains('(') && method.contains(' ') {
        "message"
    } else {
        "other"
    }
}

fn is_record_start(line: &str) -> bool {
    line.starts_with('[') && line.find("] tid:").is_some()
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from > haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| from + offset)
}

fn expect(haystack: &[u8], offset: usize, expected: &[u8]) -> Result<(), ParseError> {
    if haystack.get(offset..offset + expected.len()) == Some(expected) {
        Ok(())
    } else {
        Err(ParseError::invalid(offset, "unexpected provider header"))
    }
}

fn skip_whitespace(bytes: &[u8], mut offset: usize) -> usize {
    while offset < bytes.len() && bytes[offset].is_ascii_whitespace() {
        offset += 1;
    }
    offset
}

fn trim(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap_or_default().trim()
}

fn trim_start(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap_or_default().trim_start()
}

fn trim_separators(value: &str) -> &str {
    value.trim().trim_end_matches(';').trim()
}

fn take_i64(bytes: &[u8], mut offset: usize) -> Option<(i64, usize)> {
    let start = offset;
    let mut value = 0_i64;
    while offset < bytes.len() && bytes[offset].is_ascii_digit() {
        value = value
            .checked_mul(10)?
            .checked_add(i64::from(bytes[offset] - b'0'))?;
        offset += 1;
    }
    (offset > start).then_some((value, offset))
}

fn take_decimal(value: &str) -> Option<i64> {
    take_i64(value.as_bytes(), 0).map(|(number, _)| number)
}

fn digits_after(value: &str, prefix: &str) -> Option<i64> {
    value.strip_prefix(prefix).and_then(take_decimal)
}

fn find_marker(haystack: &[u8], marker: &[u8], from: usize) -> Option<usize> {
    let mut offset = from;
    let mut quote = None;
    let mut escaped = false;
    while offset < haystack.len() {
        let byte = haystack[offset];
        if let Some(quote_byte) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == quote_byte {
                quote = None;
            }
            offset += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
            offset += 1;
            continue;
        }
        if haystack[offset..].starts_with(marker) {
            return Some(offset);
        }
        offset += 1;
    }
    None
}

fn parser_epoch_millis(timestamp: &str) -> Option<i64> {
    let bytes = timestamp.as_bytes();
    if bytes.len() < 23 {
        return None;
    }
    let parts = [0, 5, 8, 11, 14, 17, 20];
    let separators = [
        (4, b'-'),
        (7, b'-'),
        (10, b' '),
        (13, b':'),
        (16, b':'),
        (19, b'.'),
    ];
    if separators
        .iter()
        .any(|(offset, expected)| bytes[*offset] != *expected)
    {
        return None;
    }
    let values = parts.map(|offset| take_i64(bytes, offset).map(|(value, _)| value));
    let [year, month, day, hour, minute, second, millis] = values;
    let (year, month, day, hour, minute, second, millis) =
        (year?, month?, day?, hour?, minute?, second?, millis?);
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_part = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_part + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    Some((days * 86_400 + hour * 3_600 + minute * 60 + second) * 1_000 + millis)
}
