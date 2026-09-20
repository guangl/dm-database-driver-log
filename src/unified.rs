//! 面向内置驱动格式的统一 API。

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use crate::encoding::{self, FileEncodingHint};
use crate::error::ParseError;

/// 内置日志格式的种类。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LogFormatKind {
    #[cfg(feature = "jdbc")]
    Jdbc,
    #[cfg(feature = "dm-provider")]
    DmProvider,
}

/// JDBC 或 DM Provider 日志事件。
#[derive(Debug, Clone, PartialEq)]
pub enum LogEvent {
    #[cfg(feature = "jdbc")]
    Jdbc(crate::formats::jdbc::DriverLogEvent),
    #[cfg(feature = "dm-provider")]
    DmProvider(crate::formats::dm_provider::DmProviderEvent),
}

impl LogEvent {
    /// 返回事件所属的日志格式。
    pub fn format(&self) -> LogFormatKind {
        match self {
            #[cfg(feature = "jdbc")]
            Self::Jdbc(_) => LogFormatKind::Jdbc,
            #[cfg(feature = "dm-provider")]
            Self::DmProvider(_) => LogFormatKind::DmProvider,
        }
    }

    /// 返回驱动方法或 Provider 方法名。
    pub fn method(&self) -> &str {
        match self {
            #[cfg(feature = "jdbc")]
            Self::Jdbc(event) => &event.method,
            #[cfg(feature = "dm-provider")]
            Self::DmProvider(event) => &event.method,
        }
    }

    /// 返回统一分类。
    pub fn category(&self) -> &str {
        match self {
            #[cfg(feature = "jdbc")]
            Self::Jdbc(event) => event.category,
            #[cfg(feature = "dm-provider")]
            Self::DmProvider(event) => event.category,
        }
    }

    /// 返回统一换算后的耗时（毫秒）。
    pub fn used_time_ms(&self) -> Option<f64> {
        match self {
            #[cfg(feature = "jdbc")]
            Self::Jdbc(event) => event.used_time_ms,
            #[cfg(feature = "dm-provider")]
            Self::DmProvider(event) => event.used_time_ms,
        }
    }

    /// 返回执行编号。
    pub fn exec_id(&self) -> Option<i64> {
        match self {
            #[cfg(feature = "jdbc")]
            Self::Jdbc(event) => event.exec_id,
            #[cfg(feature = "dm-provider")]
            Self::DmProvider(event) => event.exec_id,
        }
    }

    /// 返回物理起始行号；单条解析返回 0。
    pub fn line_number(&self) -> u64 {
        match self {
            #[cfg(feature = "jdbc")]
            Self::Jdbc(event) => event.line_number,
            #[cfg(feature = "dm-provider")]
            Self::DmProvider(event) => event.line_number,
        }
    }

    /// 返回记录原文。
    pub fn raw(&self) -> &str {
        match self {
            #[cfg(feature = "jdbc")]
            Self::Jdbc(event) => &event.raw,
            #[cfg(feature = "dm-provider")]
            Self::DmProvider(event) => &event.raw,
        }
    }

    /// 如果事件来自 JDBC，返回 JDBC 专属字段。
    #[cfg(feature = "jdbc")]
    pub fn as_jdbc(&self) -> Option<&crate::formats::jdbc::DriverLogEvent> {
        match self {
            Self::Jdbc(event) => Some(event),
            #[cfg(feature = "dm-provider")]
            Self::DmProvider(_) => None,
        }
    }

    /// 如果事件来自 DM Provider，返回 Provider 专属字段。
    #[cfg(feature = "dm-provider")]
    pub fn as_dm_provider(&self) -> Option<&crate::formats::dm_provider::DmProviderEvent> {
        match self {
            #[cfg(feature = "jdbc")]
            Self::Jdbc(_) => None,
            Self::DmProvider(event) => Some(event),
        }
    }
}

/// 自动识别 JDBC 或 DM Provider 格式的文件解析器构建器。
pub struct LogParserBuilder {
    path: PathBuf,
    encoding: FileEncodingHint,
    _format: PhantomData<LogEvent>,
}

impl LogParserBuilder {
    /// 创建一个统一格式解析器构建器。
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            encoding: FileEncodingHint::Auto,
            _format: PhantomData,
        }
    }

    /// 设置输入文件编码；默认自动识别 UTF-8 或 GB18030。
    pub fn encoding_hint(mut self, hint: FileEncodingHint) -> Self {
        self.encoding = hint;
        self
    }

    /// 打开文件并根据第一条记录选择内置格式。
    pub fn build(self) -> Result<LogParser, ParseError> {
        let format = detect_path_format(&self.path, self.encoding)?;
        Ok(LogParser {
            path: self.path,
            encoding: self.encoding,
            format,
        })
    }
}

/// 统一的内置驱动日志解析器。
#[derive(Debug)]
pub struct LogParser {
    path: PathBuf,
    encoding: FileEncodingHint,
    format: LogFormatKind,
}

impl LogParser {
    /// 返回自动识别出的日志格式。
    pub fn format(&self) -> LogFormatKind {
        self.format
    }

    /// 返回输入文件路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 创建一次新的流式迭代器。
    pub fn iter(&self) -> Result<LogIterator, ParseError> {
        let inner =
            match self.format {
                #[cfg(feature = "jdbc")]
                LogFormatKind::Jdbc => InnerIterator::Jdbc(
                    crate::core::LogParserBuilder::<crate::formats::jdbc::JdbcFormat>::new(
                        &self.path,
                    )
                    .encoding_hint(self.encoding)
                    .build()?
                    .iter()?,
                ),
                #[cfg(feature = "dm-provider")]
                LogFormatKind::DmProvider => {
                    InnerIterator::DmProvider(
                        crate::core::LogParserBuilder::<
                            crate::formats::dm_provider::DmProviderFormat,
                        >::new(&self.path)
                        .encoding_hint(self.encoding)
                        .build()?
                        .iter()?,
                    )
                }
            };
        Ok(LogIterator { inner })
    }
}

enum InnerIterator {
    #[cfg(feature = "jdbc")]
    Jdbc(crate::core::LogIterator<crate::formats::jdbc::JdbcFormat>),
    #[cfg(feature = "dm-provider")]
    DmProvider(crate::core::LogIterator<crate::formats::dm_provider::DmProviderFormat>),
}

/// 统一事件迭代器。
pub struct LogIterator {
    inner: InnerIterator,
}

impl LogIterator {
    /// 丢弃格式错误的记录，只返回成功解析的事件。
    pub fn skip_errors(self) -> impl Iterator<Item = LogEvent> {
        self.filter_map(Result::ok)
    }

    /// 按方法名筛选事件。
    pub fn filter_by_method(
        self,
        method: &str,
    ) -> impl Iterator<Item = Result<LogEvent, ParseError>> + '_ {
        self.filter(move |result| match result {
            Ok(event) => event.method() == method,
            Err(_) => true,
        })
    }

    /// 按分类筛选事件。
    pub fn filter_by_category(
        self,
        category: &str,
    ) -> impl Iterator<Item = Result<LogEvent, ParseError>> + '_ {
        self.filter(move |result| match result {
            Ok(event) => event.category() == category,
            Err(_) => true,
        })
    }

    /// 按最小耗时（毫秒）筛选事件。
    pub fn filter_by_used_time(
        self,
        min_ms: f64,
    ) -> impl Iterator<Item = Result<LogEvent, ParseError>> {
        self.filter(move |result| match result {
            Ok(event) => event.used_time_ms().is_some_and(|value| value >= min_ms),
            Err(_) => true,
        })
    }

    /// 按执行编号筛选事件。
    pub fn filter_by_exec_id(
        self,
        exec_id: i64,
    ) -> impl Iterator<Item = Result<LogEvent, ParseError>> {
        self.filter(move |result| match result {
            Ok(event) => event.exec_id() == Some(exec_id),
            Err(_) => true,
        })
    }
}

impl Iterator for LogIterator {
    type Item = Result<LogEvent, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            #[cfg(feature = "jdbc")]
            InnerIterator::Jdbc(iterator) => {
                iterator.next().map(|result| result.map(LogEvent::Jdbc))
            }
            #[cfg(feature = "dm-provider")]
            InnerIterator::DmProvider(iterator) => iterator
                .next()
                .map(|result| result.map(LogEvent::DmProvider)),
        }
    }
}

/// 自动识别并解析一条 JDBC 或 DM Provider 记录。
pub fn parse_line(line: &str) -> Result<LogEvent, ParseError> {
    let format = detect_line_format(line)
        .or_else(default_format)
        .ok_or_else(|| ParseError::invalid(0, "unknown driver log format").with_context(line, 0))?;
    parse_line_with_format(line, format)
}

/// 自动识别并解析一条字节记录。
pub fn parse_bytes(line: &[u8]) -> Result<LogEvent, ParseError> {
    parse_bytes_with_encoding(line, FileEncodingHint::Auto)
}

/// 按指定编码自动识别并解析一条字节记录。
pub fn parse_bytes_with_encoding(
    line: &[u8],
    encoding: FileEncodingHint,
) -> Result<LogEvent, ParseError> {
    parse_line(&encoding::decode(line, encoding))
}

fn parse_line_with_format(line: &str, format: LogFormatKind) -> Result<LogEvent, ParseError> {
    match format {
        #[cfg(feature = "jdbc")]
        LogFormatKind::Jdbc => {
            crate::formats::jdbc::parse(line).map(|event| LogEvent::Jdbc(event.to_owned(line, 0)))
        }
        #[cfg(feature = "dm-provider")]
        LogFormatKind::DmProvider => {
            crate::formats::dm_provider::parse_line(line).map(LogEvent::DmProvider)
        }
    }
}

fn detect_path_format(
    path: &Path,
    encoding: FileEncodingHint,
) -> Result<LogFormatKind, ParseError> {
    let file = File::open(path)
        .map_err(|error| ParseError::IoError(format!("{}: {error}", path.display())))?;
    let mut reader = BufReader::new(file);
    let mut bytes = Vec::new();
    let mut line_number = 0;

    loop {
        bytes.clear();
        let bytes_read = reader
            .read_until(b'\n', &mut bytes)
            .map_err(|error| ParseError::IoError(error.to_string()))?;
        if bytes_read == 0 {
            return default_format()
                .ok_or_else(|| ParseError::invalid(0, "no built-in driver log format is enabled"));
        }
        line_number += 1;
        while matches!(bytes.last(), Some(b'\n' | b'\r')) {
            bytes.pop();
        }
        let line = encoding::decode(&bytes, encoding);
        if line.trim().is_empty() {
            continue;
        }
        return detect_line_format(&line).ok_or_else(|| {
            ParseError::invalid(0, "unknown driver log format").with_context(&line, line_number)
        });
    }
}

fn detect_line_format(line: &str) -> Option<LogFormatKind> {
    #[cfg(all(feature = "jdbc", feature = "dm-provider"))]
    {
        let header_end = line.find(']')?;
        let suffix = &line[header_end + 1..];
        if suffix.contains(" - [") {
            return Some(LogFormatKind::Jdbc);
        }
        if suffix.contains(" (") {
            return Some(LogFormatKind::DmProvider);
        }
        None
    }

    #[cfg(all(feature = "jdbc", not(feature = "dm-provider")))]
    {
        let _ = line;
        Some(LogFormatKind::Jdbc)
    }

    #[cfg(all(feature = "dm-provider", not(feature = "jdbc")))]
    {
        let _ = line;
        Some(LogFormatKind::DmProvider)
    }
}

fn default_format() -> Option<LogFormatKind> {
    #[cfg(feature = "jdbc")]
    {
        Some(LogFormatKind::Jdbc)
    }
    #[cfg(all(not(feature = "jdbc"), feature = "dm-provider"))]
    {
        Some(LogFormatKind::DmProvider)
    }
    #[cfg(not(any(feature = "jdbc", feature = "dm-provider")))]
    {
        None
    }
}
