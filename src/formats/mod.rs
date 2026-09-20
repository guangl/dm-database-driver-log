//! 内置驱动日志格式适配器。
//!
//! 内置格式通过 crate 内部的通用解析引擎实现；其他驱动格式请 fork 源码后扩展本目录。

#[cfg(feature = "dm-provider")]
pub mod dm_provider;
#[cfg(feature = "jdbc")]
pub mod jdbc;
