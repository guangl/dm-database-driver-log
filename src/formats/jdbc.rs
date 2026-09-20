//! DM（达梦）JDBC 驱动日志行的解析器。
//!
//! 设计约束：不使用正则表达式，全部是单次线性扫描；除极少数需要拼接的字段外不分配内存。
//!
//! 单行结构（两种形态）：
//!
//! ```text
//! [LEVEL - ts] tid:N - [thread] { conn-x, pstmt-y, rs-z } method(args): ret;  [PARAMS]: p;  [USED TIME]: t ms; [EXEC_ID]: e;
//! [DEBUG - ts] tid:N - [thread] { conn-x } access();  CMD_EXECUTE2
//! ```
//!
//! 三个关键点保证不用正则也能切得准：
//!   1. 头部按固定分隔符逐段前进（`']'` -> `'tid:'` -> `'-'` -> `'[thread]'` -> `'{ids}'`），失败即报错，不回溯。
//!   2. 尾部标记（`[PARAMS]` / `[USED TIME]` / `[EXEC_ID]` / `CMD_x`）必须出现在「分号 + 若干空格」之后，
//!      因此列值、SQL 文本里即使含有分号或方括号也不会被误切。
//!   3. 字段终点由「下一个标记的起点」决定，天然避开值里含分号的情况（例如 SQL 里的 ';'）。

use crate::core::{LogFormat, RecordFraming};
use crate::error::ParseError;

/// 一行日志解析出的结构化事件（字段全部借用原始行，零拷贝）。
#[derive(Debug, Clone, Default)]
pub struct Event<'a> {
    pub level: &'a str,
    pub event_time_text: &'a str,
    /// 由 event_time_text 换算出的 epoch 毫秒
    pub event_time_ms: Option<i64>,
    pub tid: Option<i32>,
    pub thread: &'a str,
    pub conn_id: Option<i32>,
    pub pstmt_id: Option<i32>,
    pub rs_id: Option<i32>,
    /// 统一口径的结果集编号：executeQuery 行取返回值里的 rs-N，其余行取 id 块里的 rs-N
    pub result_set_id: Option<i32>,
    pub handle_id: Option<i32>,
    pub session_id_hex: Option<&'a str>,
    pub method: &'a str,
    pub arg_types: &'a str,
    pub return_value: Option<&'a str>,
    pub params: Option<&'a str>,
    pub param_index: Option<i32>,
    pub param_value: Option<&'a str>,
    pub column_name: Option<&'a str>,
    pub used_time_text: Option<&'a str>,
    pub used_time_ms: Option<f64>,
    pub exec_id: Option<i64>,
    pub cmd: Option<&'a str>,
    pub category: &'static str,
}

impl<'a> Event<'a> {
    /// 将当前行的零拷贝事件复制成可脱离输入缓冲区保存的记录。
    pub fn to_owned(&self, raw: &str, line_number: u64) -> DriverLogEvent {
        DriverLogEvent {
            line_number,
            raw: raw.to_owned(),
            level: self.level.to_owned(),
            event_time_text: self.event_time_text.to_owned(),
            event_time_ms: self.event_time_ms,
            tid: self.tid,
            thread: self.thread.to_owned(),
            conn_id: self.conn_id,
            pstmt_id: self.pstmt_id,
            rs_id: self.rs_id,
            result_set_id: self.result_set_id,
            handle_id: self.handle_id,
            session_id_hex: self.session_id_hex.map(str::to_owned),
            method: self.method.to_owned(),
            arg_types: self.arg_types.to_owned(),
            return_value: self.return_value.map(str::to_owned),
            params: self.params.map(str::to_owned),
            param_index: self.param_index,
            param_value: self.param_value.map(str::to_owned),
            column_name: self.column_name.map(str::to_owned),
            used_time_text: self.used_time_text.map(str::to_owned),
            used_time_ms: self.used_time_ms,
            exec_id: self.exec_id,
            cmd: self.cmd.map(str::to_owned),
            category: self.category,
        }
    }
}

/// 一条可独立保存的驱动日志事件。
///
/// 文件迭代器返回此类型；统一入口会将 JDBC 记录包装为
/// [`crate::LogEvent::Jdbc`]。
#[derive(Debug, Clone, PartialEq)]
pub struct DriverLogEvent {
    pub line_number: u64,
    pub raw: String,
    pub level: String,
    pub event_time_text: String,
    pub event_time_ms: Option<i64>,
    pub tid: Option<i32>,
    pub thread: String,
    pub conn_id: Option<i32>,
    pub pstmt_id: Option<i32>,
    pub rs_id: Option<i32>,
    pub result_set_id: Option<i32>,
    pub handle_id: Option<i32>,
    pub session_id_hex: Option<String>,
    pub method: String,
    pub arg_types: String,
    pub return_value: Option<String>,
    pub params: Option<String>,
    pub param_index: Option<i32>,
    pub param_value: Option<String>,
    pub column_name: Option<String>,
    pub used_time_text: Option<String>,
    pub used_time_ms: Option<f64>,
    pub exec_id: Option<i64>,
    pub cmd: Option<String>,
    pub category: &'static str,
}

/// JDBC 日志格式适配器。
#[derive(Copy, Clone, Debug, Default)]
pub struct JdbcFormat;

impl LogFormat for JdbcFormat {
    type Event = DriverLogEvent;

    const FRAMING: RecordFraming = RecordFraming::Line;

    fn is_record_start(line: &str) -> bool {
        line.starts_with('[')
    }

    fn parse_record(record: &str, line_number: u64) -> Result<Self::Event, ParseError> {
        parse(record).map(|event| event.to_owned(record, line_number))
    }
}

const MARK_PARAMS: &[u8] = b"[PARAMS]: ";
const MARK_USED_TIME: &[u8] = b"[USED TIME]: ";
const MARK_EXEC_ID: &[u8] = b"[EXEC_ID]: ";
const MARK_CMD: &[u8] = b"CMD_";
const MARK_SESSION: &[u8] = b"sessionID-";

/// 解析一行（不含换行符）。line 必须是合法 UTF-8（调用方校验），
/// 这样所有切片都落在字符边界上——所有切分点都是 ASCII。
pub fn parse(line: &str) -> Result<Event<'_>, ParseError> {
    parse_inner(line).map_err(|e| e.with_context(line, 0))
}

fn parse_inner(line: &str) -> Result<Event<'_>, ParseError> {
    let b = line.as_bytes();
    if b.is_empty() {
        return Err(ParseError::invalid(0, "empty line"));
    }
    if b[0] != b'[' {
        return Err(ParseError::invalid(0, "line does not start with '['"));
    }

    // ---- 头部：[LEVEL - ts] ----
    let head_end = find(b, b"]", 1).ok_or(ParseError::invalid(1, "unterminated level header"))?;
    let head = &b[1..head_end];
    let sep = find(head, b" - ", 0).ok_or(ParseError::invalid(1, "missing ' - ' in header"))?;
    let level = trim(&head[..sep]);
    let ts = trim(&head[sep + 3..]);

    // ---- tid / thread / id 块 ----
    let mut i = skip_spaces(b, head_end + 1);
    expect(b, i, b"tid:")?;
    i += 4;
    let (tid, ni) = take_i64(b, i).ok_or(ParseError::invalid(i, "bad tid"))?;
    i = skip_spaces(b, ni);
    expect(b, i, b"-")?;
    i = skip_spaces(b, i + 1);
    expect(b, i, b"[")?;
    let thread_end = find(b, b"]", i + 1).ok_or(ParseError::invalid(i, "unterminated thread"))?;
    let thread = trim(&b[i + 1..thread_end]);
    i = skip_spaces(b, thread_end + 1);
    expect(b, i, b"{")?;
    let ids_end = find(b, b"}", i + 1).ok_or(ParseError::invalid(i, "unterminated id block"))?;
    let ids = trim(&b[i + 1..ids_end]);
    let rest = trim_start(&b[ids_end + 1..]);
    let rb = rest.as_bytes();

    // ---- method(args) ----
    let popen = find(rb, b"(", 0).ok_or(ParseError::invalid(0, "missing '(' after method"))?;
    let method = trim(&rb[..popen]);
    let pclose =
        find(rb, b")", popen + 1).ok_or(ParseError::invalid(popen, "unterminated method args"))?;
    let arg_types = trim(&rb[popen + 1..pclose]);
    let after = pclose + 1;

    // ---- 尾部标记（只在字段边界上匹配）----
    let m_params = find_marker(rb, MARK_PARAMS, after);
    let m_time = find_marker(rb, MARK_USED_TIME, after);
    let m_exec = find_marker(rb, MARK_EXEC_ID, after);
    let m_cmd = find_marker(rb, MARK_CMD, after);
    let first_tail = [m_params, m_time, m_exec, m_cmd]
        .into_iter()
        .flatten()
        .min();

    let mut ev = Event {
        level,
        event_time_text: ts,
        event_time_ms: epoch_millis(ts),
        tid: i32::try_from(tid).ok(),
        thread,
        method,
        arg_types,
        category: category_of(method),
        ..Default::default()
    };

    // 返回值：只有 "): x" 形态才有；") ;" 表示 void 方法
    if rb.get(after) == Some(&b':') {
        let end = first_tail.unwrap_or(rb.len());
        if end > after + 1 {
            let raw = trim_semi(trim(&rb[after + 1..end]));
            if !raw.is_empty() {
                ev.return_value = Some(raw);
            }
        }
    }

    // [PARAMS]: 直到下一个标记
    if let Some(p) = m_params {
        let start = p + MARK_PARAMS.len();
        let end = [m_time, m_exec, m_cmd]
            .into_iter()
            .flatten()
            .filter(|x| *x > start)
            .min()
            .unwrap_or(rb.len());
        if end > start {
            let raw = trim_semi(trim(&rb[start..end]));
            if !raw.is_empty() {
                ev.params = Some(raw);
            }
        }
    }

    // [USED TIME]: 1.23ms / 4.5E-4ms
    if let Some(p) = m_time {
        let start = p + MARK_USED_TIME.len();
        if let Some(m) = find(rb, b"ms", start) {
            let num = trim(&rb[start..m]);
            if !num.is_empty() {
                ev.used_time_text = Some(num);
                ev.used_time_ms = num.parse::<f64>().ok();
            }
        }
    }

    // [EXEC_ID]: 19010657
    if let Some(p) = m_exec
        && let Some((v, _)) = take_i64(rb, p + MARK_EXEC_ID.len())
    {
        ev.exec_id = Some(v);
    }

    // CMD_EXECUTE2 / CMD_FETCH / CMD_COMMIT
    if let Some(p) = m_cmd {
        let seg = &rb[p..];
        let mut e = 0;
        while e < seg.len()
            && (seg[e].is_ascii_uppercase() || seg[e].is_ascii_digit() || seg[e] == b'_')
        {
            e += 1;
        }
        if e > MARK_CMD.len() {
            ev.cmd = rest.get(p..p + e);
        }
    }

    // ---- id 块：conn-3, pstmt-854, rs-2216 ----
    for token in ids.split(',') {
        let t = trim(token.as_bytes());
        let tb = t.as_bytes();
        if let Some(v) = strip_digits(tb, b"conn-") {
            ev.conn_id = i32::try_from(v).ok();
        } else if let Some(v) = strip_digits(tb, b"pstmt-") {
            ev.pstmt_id = i32::try_from(v).ok();
        } else if let Some(v) = strip_digits(tb, b"rs-") {
            ev.rs_id = i32::try_from(v).ok();
        } else if let Some(v) = strip_digits(tb, b"handle-") {
            ev.handle_id = i32::try_from(v).ok();
        }
    }

    // ---- 返回值里的对象编号 ----
    if let Some(rv) = ev.return_value {
        let vrb = rv.as_bytes();
        if let Some(v) = digits_after(vrb, b"rs-") {
            // executeQuery(): rs-2216
            ev.result_set_id = i32::try_from(v).ok();
        } else if let Some(v) = digits_after(vrb, b"pstmt-") {
            // prepareStatement(): pstmt-854, handle-3, sessionID-0x6c0504a0(1812268192)
            ev.pstmt_id = i32::try_from(v).ok();
            if let Some(h) = find(vrb, b"handle-", 0).and_then(|i| take_i64(vrb, i + 7)) {
                ev.handle_id = i32::try_from(h.0).ok();
            }
            if let Some(s) = find(vrb, MARK_SESSION, 0) {
                let start = s + MARK_SESSION.len();
                let seg = &vrb[start..];
                let mut e = 0;
                while e < seg.len()
                    && (seg[e].is_ascii_hexdigit() || seg[e] == b'x' || seg[e] == b'X')
                {
                    e += 1;
                }
                ev.session_id_hex = rv.get(start..start + e);
            }
        }
    }
    ev.result_set_id = ev.result_set_id.or(ev.rs_id);

    // ---- 绑定参数（setXxx）：PARAMS 形如  1, "A06093910597367000594686" ----
    if ev.category == "bind"
        && let Some(p) = ev.params
    {
        let pb = p.as_bytes();
        if let Some((idx, ni)) = take_i64(pb, 0) {
            ev.param_index = i32::try_from(idx).ok();
            let mut value_start = ni.min(pb.len());
            while value_start < pb.len()
                && (pb[value_start] == b',' || pb[value_start].is_ascii_whitespace())
            {
                value_start += 1;
            }
            let v = trim(&pb[value_start..]);
            if !v.is_empty() {
                ev.param_value = Some(v);
            }
        }
    }

    // ---- 读取列（getXxx）：PARAMS 形如  "id_pirec1_119_" ----
    if ev.method.starts_with("get")
        && let Some(p) = ev.params
        && p.len() >= 2
        && p.starts_with('"')
        && p.ends_with('"')
    {
        ev.column_name = Some(&p[1..p.len() - 1]);
    }

    Ok(ev)
}

/// 方法名 -> 事件分类。
pub fn category_of(method: &str) -> &'static str {
    if method.is_empty() {
        return "unparsed";
    }
    if method == "prepareStatement" {
        return "prepare";
    }
    if method.starts_with("set") {
        return "bind";
    }
    if method.starts_with("execute") {
        return "execute";
    }
    if method == "next" {
        return "fetch_next";
    }
    if method == "wasNull" {
        return "null_check";
    }
    if method.starts_with("clear") {
        return "clear";
    }
    if method == "close" {
        return "close";
    }
    if method == "access" {
        return "driver_access";
    }
    if method.starts_with("get") {
        return match method {
            "getString" | "getTimestamp" | "getLong" | "getInt" | "getShort" | "getDouble"
            | "getFloat" | "getBoolean" | "getBytes" | "getBigDecimal" | "getDate" | "getTime"
            | "getObject" | "getBlob" | "getClob" | "getNString" | "getNClob" | "getURL"
            | "getAsciiStream" | "getBinaryStream" | "getCharacterStream" => "read_value",
            _ => "read_meta",
        };
    }
    "other"
}

// ------------------------------------------------------------------ 基础扫描工具

/// 朴素子串查找：先定位首字节再逐字节比较。样例日志行很短（p99 = 197 字节），
/// 无需 BM/KMP，实测比正则快一到两个数量级。
pub fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() || from > hay.len() {
        return None;
    }
    let first = needle[0];
    let last_start = hay.len() - needle.len();
    let mut i = from;
    while i <= last_start {
        if hay[i] == first && &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// 只在字段边界且不在引号字符串内匹配标记：标记左边必须是分号
/// （中间允许空格）。这样 SQL 参数即使包含 `; [USED TIME]` 也不会被误切。
fn find_marker(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    while i < hay.len() {
        let ch = hay[i];
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == b'\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if ch == b'\'' || ch == b'"' {
            quote = Some(ch);
            i += 1;
            continue;
        }
        if hay[i..].starts_with(needle) && is_field_boundary(hay, i) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn is_field_boundary(b: &[u8], pos: usize) -> bool {
    let mut j = pos;
    while j > 0 && b[j - 1] == b' ' {
        j -= 1;
    }
    j > 0 && b[j - 1] == b';'
}

fn skip_spaces(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    i
}

fn expect(b: &[u8], i: usize, lit: &[u8]) -> Result<(), ParseError> {
    if b.len() >= i + lit.len() && &b[i..i + lit.len()] == lit {
        Ok(())
    } else {
        let what = match lit {
            b"tid:" => "expected 'tid:'",
            b"-" => "expected '-'",
            b"[" => "expected '['",
            b"{" => "expected '{'",
            _ => "unexpected token",
        };
        Err(ParseError::invalid(i, what))
    }
}

/// 去掉首尾空白。切分点均为 ASCII，故一定是合法 UTF-8 边界。
pub fn trim(b: &[u8]) -> &str {
    let mut s = 0;
    let mut e = b.len();
    while s < e && (b[s] == b' ' || b[s] == b'\t' || b[s] == b'\r' || b[s] == b'\n') {
        s += 1;
    }
    while e > s && (b[e - 1] == b' ' || b[e - 1] == b'\t' || b[e - 1] == b'\r' || b[e - 1] == b'\n')
    {
        e -= 1;
    }
    unsafe { std::str::from_utf8_unchecked(&b[s..e]) }
}

fn trim_start(b: &[u8]) -> &str {
    let mut s = 0;
    while s < b.len() && (b[s] == b' ' || b[s] == b'\t') {
        s += 1;
    }
    unsafe { std::str::from_utf8_unchecked(&b[s..]) }
}

/// 去掉行尾的分号与空白（字段的收尾符）。
fn trim_semi(s: &str) -> &str {
    let b = s.as_bytes();
    let mut e = b.len();
    while e > 0 && (b[e - 1] == b';' || b[e - 1] == b' ') {
        e -= 1;
    }
    &s[..e]
}

/// 读取从 i 开始的连续十进制数字。
fn take_i64(b: &[u8], i: usize) -> Option<(i64, usize)> {
    let mut j = i;
    let mut v: i64 = 0;
    while j < b.len() && b[j].is_ascii_digit() {
        v = v.checked_mul(10)?.checked_add((b[j] - b'0') as i64)?;
        j += 1;
    }
    if j == i { None } else { Some((v, j)) }
}

/// prefix + 十进制数字，且必须整段匹配（用于 id 块里的 token）。
fn strip_digits(b: &[u8], prefix: &[u8]) -> Option<i64> {
    if b.len() > prefix.len() && b.starts_with(prefix) {
        let (v, end) = take_i64(b, prefix.len())?;
        if end == b.len() { Some(v) } else { None }
    } else {
        None
    }
}

/// prefix + 十进制数字，允许后面还有别的内容（用于返回值）。
fn digits_after(b: &[u8], prefix: &[u8]) -> Option<i64> {
    if b.starts_with(prefix) {
        take_i64(b, prefix.len()).map(|(v, _)| v)
    } else {
        None
    }
}

/// YYYY-MM-DD HH:MM:SS.mmm -> epoch 毫秒（按 UTC 解释，无时区）。
pub fn epoch_millis(ts: &str) -> Option<i64> {
    let b = ts.as_bytes();
    if b.len() < 23 {
        return None;
    }
    if b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b' '
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'.'
    {
        return None;
    }
    let num = |off: usize, len: usize| -> Option<i64> {
        take_i64(b, off).and_then(|(v, end)| if end == off + len { Some(v) } else { None })
    };
    let (y, mo, d) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (h, mi, s) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    let ms = num(20, 3)?;
    let days = days_from_civil(y, mo, d);
    Some((days * 86_400 + h * 3_600 + mi * 60 + s) * 1_000 + ms)
}

/// Howard Hinnant 的 civil -> days 算法（1970-01-01 记为 0）。
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
