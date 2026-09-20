//! DM（达梦）驱动日志解析库。
//!
//! 驱动日志是一行一个事件，常见形式如下：
//!
//! ```text
//! [INFO - 2026-09-16 17:45:19.763] tid:119 - [worker] { conn-3, pstmt-854 } executeQuery(): rs-2216; [USED TIME]: 8.5ms; [EXEC_ID]: 19010657;
//! ```
//!
//! 内置 JDBC 与 DM Provider 格式通过统一的 [`LogParserBuilder`] 自动识别。
//! 其他驱动格式请 fork 源码后在内部格式模块中实现。

mod core;
mod encoding;
mod error;
mod formats;
#[cfg(any(feature = "jdbc", feature = "dm-provider"))]
mod unified;

pub use encoding::FileEncodingHint;
pub use error::ParseError;
#[cfg(any(feature = "jdbc", feature = "dm-provider"))]
pub use unified::{
    LogEvent, LogFormatKind, LogIterator, LogParser, LogParserBuilder, parse_bytes,
    parse_bytes_with_encoding, parse_line,
};

#[cfg(feature = "dm-provider")]
pub use formats::dm_provider::DmProviderEvent;
#[cfg(feature = "jdbc")]
pub use formats::jdbc::DriverLogEvent as JdbcEvent;
