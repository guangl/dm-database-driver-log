//! 所有驱动日志格式共用的文件流式解析引擎。
//!
//! 具体格式只需要实现 [`LogFormat`]，文件生命周期、编码、记录 framing、错误上下文
//! 和通用过滤器都由这里统一处理。

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use crate::encoding::{self, FileEncodingHint};
use crate::error::ParseError;

/// 输入文件中一条记录的边界策略。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RecordFraming {
    /// 每个物理行都是一条记录。
    Line,
    /// 以格式定义的记录头开始，直到下一条记录头之间的内容属于同一条记录。
    HeaderDelimited,
}

/// 通用事件字段访问接口，供格式无关的过滤器使用。
pub trait LogRecord {
    fn method(&self) -> &str;
    fn category(&self) -> &str;
    fn used_time_ms(&self) -> Option<f64>;
    fn exec_id(&self) -> Option<i64>;
}

/// 一个可插拔的驱动日志格式适配器。
///
/// 新增格式时只需要实现这个 trait，并为事件实现 [`LogRecord`]；文件迭代和
/// 通用过滤器无需重新实现。
pub trait LogFormat: 'static {
    type Event: LogRecord;

    const FRAMING: RecordFraming;

    /// 判断一条物理行是否是该格式的记录头。
    fn is_record_start(line: &str) -> bool;

    /// 将一条已按 framing 合并的原始记录转换为拥有数据的事件。
    /// 实现应将 `line_number` 和 `raw` 写入事件；解析错误可以只描述格式，
    /// 通用引擎会补充原文和文件行号。
    fn parse_record(record: &str, line_number: u64) -> Result<Self::Event, ParseError>;
}

/// 通用的日志文件解析器。
#[derive(Debug)]
pub struct LogParser<F: LogFormat> {
    path: PathBuf,
    encoding: FileEncodingHint,
    _format: PhantomData<F>,
}

impl<F: LogFormat> LogParser<F> {
    /// 每次调用都会重新打开文件，适合重复执行不同的过滤查询。
    pub fn iter(&self) -> Result<LogIterator<F>, ParseError> {
        let file = File::open(&self.path)
            .map_err(|error| ParseError::IoError(format!("{}: {error}", self.path.display())))?;
        Ok(LogIterator {
            reader: BufReader::with_capacity(1 << 20, file),
            encoding: self.encoding,
            line_number: 0,
            line_buf: Vec::with_capacity(4096),
            lookahead: None,
            done: false,
            _format: PhantomData,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// 通用日志文件解析器构建器。
pub struct LogParserBuilder<F: LogFormat> {
    path: PathBuf,
    encoding: FileEncodingHint,
    _format: PhantomData<F>,
}

impl<F: LogFormat> LogParserBuilder<F> {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            encoding: FileEncodingHint::Auto,
            _format: PhantomData,
        }
    }

    /// 设置输入文件编码；默认使用 [`FileEncodingHint::Auto`]。
    pub fn encoding_hint(mut self, hint: FileEncodingHint) -> Self {
        self.encoding = hint;
        self
    }

    /// 打开并验证输入文件，然后构建一个可重复调用 `iter()` 的解析器。
    pub fn build(self) -> Result<LogParser<F>, ParseError> {
        let path = self.path;
        match File::open(&path) {
            Ok(_) => Ok(LogParser {
                path,
                encoding: self.encoding,
                _format: PhantomData,
            }),
            Err(error) => Err(ParseError::IoError(format!("{}: {error}", path.display()))),
        }
    }
}

/// 通用日志记录迭代器。
pub struct LogIterator<F: LogFormat> {
    reader: BufReader<File>,
    encoding: FileEncodingHint,
    line_number: u64,
    line_buf: Vec<u8>,
    lookahead: Option<(u64, String)>,
    done: bool,
    _format: PhantomData<F>,
}

impl<F: LogFormat> LogIterator<F> {
    /// 丢弃格式错误的记录，只返回成功解析的事件。
    pub fn skip_errors(self) -> impl Iterator<Item = F::Event> {
        self.filter_map(Result::ok)
    }

    /// 只保留指定方法的事件。
    pub fn filter_by_method(
        self,
        method: &str,
    ) -> impl Iterator<Item = Result<F::Event, ParseError>> + '_ {
        self.filter(move |result| match result {
            Ok(event) => event.method() == method,
            Err(_) => true,
        })
    }

    /// 只保留指定分类的事件。
    pub fn filter_by_category(
        self,
        category: &str,
    ) -> impl Iterator<Item = Result<F::Event, ParseError>> + '_ {
        self.filter(move |result| match result {
            Ok(event) => event.category() == category,
            Err(_) => true,
        })
    }

    /// 只保留驱动耗时大于等于 `min_ms` 的事件。
    pub fn filter_by_used_time(
        self,
        min_ms: f64,
    ) -> impl Iterator<Item = Result<F::Event, ParseError>> {
        self.filter(move |result| match result {
            Ok(event) => event.used_time_ms().is_some_and(|value| value >= min_ms),
            Err(_) => true,
        })
    }

    /// 只保留属于指定执行编号的事件。
    pub fn filter_by_exec_id(
        self,
        exec_id: i64,
    ) -> impl Iterator<Item = Result<F::Event, ParseError>> {
        self.filter(move |result| match result {
            Ok(event) => event.exec_id() == Some(exec_id),
            Err(_) => true,
        })
    }

    fn read_line(&mut self) -> Result<Option<(u64, String)>, ParseError> {
        self.line_buf.clear();
        let bytes_read = self
            .reader
            .read_until(b'\n', &mut self.line_buf)
            .map_err(|error| ParseError::IoError(error.to_string()))?;
        if bytes_read == 0 {
            return Ok(None);
        }

        self.line_number += 1;
        let mut end = self.line_buf.len();
        while end > 0 && matches!(self.line_buf[end - 1], b'\n' | b'\r') {
            end -= 1;
        }
        Ok(Some((
            self.line_number,
            encoding::decode(&self.line_buf[..end], self.encoding),
        )))
    }

    fn parse_record(&self, record: &str, line_number: u64) -> Result<F::Event, ParseError> {
        F::parse_record(record, line_number)
            .map_err(|error| error.with_context(record, line_number))
    }

    fn next_line_record(&mut self) -> Option<Result<F::Event, ParseError>> {
        loop {
            let (line_number, line) = match self.read_line() {
                Ok(Some(line)) => line,
                Ok(None) => {
                    self.done = true;
                    return None;
                }
                Err(error) => {
                    self.done = true;
                    return Some(Err(error));
                }
            };
            if line.is_empty() {
                continue;
            }
            return Some(self.parse_record(&line, line_number));
        }
    }

    fn next_header_delimited_record(&mut self) -> Option<Result<F::Event, ParseError>> {
        let mut record = String::new();
        let mut start_line = 0;

        loop {
            let next_line = match self.lookahead.take() {
                Some(line) => Some(line),
                None => match self.read_line() {
                    Ok(line) => line,
                    Err(error) => {
                        self.done = true;
                        return Some(Err(error));
                    }
                },
            };
            let Some((line_number, line)) = next_line else {
                self.done = true;
                if record.is_empty() {
                    return None;
                }
                break;
            };

            if F::is_record_start(&line) && !record.is_empty() {
                self.lookahead = Some((line_number, line));
                break;
            }
            if record.is_empty() {
                if line.is_empty() {
                    continue;
                }
                start_line = line_number;
                record = line;
            } else {
                record.push('\n');
                record.push_str(&line);
            }
        }

        Some(self.parse_record(&record, start_line))
    }
}

impl<F: LogFormat> Iterator for LogIterator<F> {
    type Item = Result<F::Event, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        match F::FRAMING {
            RecordFraming::Line => self.next_line_record(),
            RecordFraming::HeaderDelimited => self.next_header_delimited_record(),
        }
    }
}
