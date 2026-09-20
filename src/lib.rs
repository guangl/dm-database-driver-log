//! DM（达梦）驱动日志解析库。
//!
//! 驱动日志是一行一个事件，常见形式如下：
//!
//! ```text
//! [INFO - 2026-09-16 17:45:19.763] tid:119 - [worker] { conn-3, pstmt-854 } executeQuery(): rs-2216; [USED TIME]: 8.5ms; [EXEC_ID]: 19010657;
//! ```
//!
//! 具体格式实现 [`LogFormat`] 和 [`LogRecord`]，文件读取与过滤由通用
//! [`LogParserBuilder`] 提供；JDBC 和 DM Provider 是内置适配器。

mod core;
mod encoding;
mod error;
mod formats;

pub use core::{LogFormat, LogIterator, LogParser, LogParserBuilder, LogRecord, RecordFraming};
pub use encoding::FileEncodingHint;
pub use error::ParseError;
#[cfg(feature = "dm-provider")]
pub use formats::dm_provider;
#[cfg(feature = "dm-provider")]
pub use formats::dm_provider::{
    DmProviderEvent, DmProviderLogIterator, DmProviderLogParser, DmProviderLogParserBuilder,
};
#[cfg(feature = "jdbc")]
pub use formats::jdbc as parser;
#[cfg(feature = "jdbc")]
pub use formats::jdbc::{
    DriverLogEvent, DriverLogIterator, DriverLogParser, DriverLogParserBuilder, Event,
};

#[cfg(feature = "jdbc")]
/// 解析一行并返回拥有自身字符串数据的事件。
///
/// 直接调用 [`parser::parse`] 可避免这次分配；该函数更适合需要跨越输入
/// 缓冲区保存结果的调用方。返回记录的 `line_number` 为 0，文件行号由迭代器填充。
pub fn parse_line(line: &str) -> Result<DriverLogEvent, ParseError> {
    formats::jdbc::parse(line).map(|event| event.to_owned(line, 0))
}

#[cfg(feature = "jdbc")]
/// 从字节切片解析一行。非 UTF-8 字节按 lossy UTF-8 解码，保证驱动日志中
/// 混入少量本地编码字符时仍能保留可查询的事件结构。
pub fn parse_bytes(line: &[u8]) -> Result<DriverLogEvent, ParseError> {
    parse_bytes_with_encoding(line, FileEncodingHint::Auto)
}

#[cfg(feature = "jdbc")]
/// 按指定编码解析一行字节。
pub fn parse_bytes_with_encoding(
    line: &[u8],
    encoding: FileEncodingHint,
) -> Result<DriverLogEvent, ParseError> {
    let text = encoding::decode(line, encoding);
    parse_line(&text)
}
