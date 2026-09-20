//! 内置驱动日志格式适配器。
//!
//! 新增驱动格式时，在本目录创建一个模块，实现 [`crate::LogFormat`] 和
//! [`crate::LogRecord`]，再在 `Cargo.toml` 的 feature 与这里注册即可。

#[cfg(feature = "dm-provider")]
pub mod dm_provider;
#[cfg(feature = "jdbc")]
pub mod jdbc;
