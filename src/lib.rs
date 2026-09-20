//! DM（达梦）驱动日志解析库。
//!
//! 驱动日志是一行一个事件，常见形式如下：
//!
//! ```text
//! [INFO - 2026-09-16 17:45:19.763] tid:119 - [worker] { conn-3, pstmt-854 } executeQuery(): rs-2216; [USED TIME]: 8.5ms; [EXEC_ID]: 19010657;
//! ```
//!
//! 内置 JDBC 与 DM Provider 格式通过统一的 [`LogParserBuilder`] 自动识别。
//! 其他驱动可以使用 [`advanced`] 命名空间中的通用格式引擎扩展。

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

/// 面向新增驱动格式的高级通用引擎 API。
pub mod advanced {
    pub use crate::core::{
        LogFormat, LogIterator, LogParser, LogParserBuilder, LogRecord, RecordFraming,
    };
}

#[cfg(feature = "dm-provider")]
pub use formats::dm_provider::DmProviderEvent;
#[cfg(feature = "jdbc")]
pub use formats::jdbc::DriverLogEvent as JdbcEvent;
