# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] - Unreleased

### Changed (Breaking)

- 移除公开的 `advanced` 通用引擎 API。
- 其他驱动格式需要 fork 源码后在内部格式模块中实现，不再承诺通过 crate 公共 API 扩展。

## [0.1.1] - 2026-09-20

### Fixed

- 修复 `perf_full` 示例在 Rust 1.85 MSRV 下无法通过 Clippy 的问题。

### Added

- 在 crate metadata 中明确声明 Rust 1.85 为最低支持版本。
- 完善 GitHub Actions 的测试、覆盖率、Clippy、文档和 benchmark 门禁。

## [0.1.0] - 2026-09-20

### Added

- 提供 JDBC 与 DM Provider 驱动日志的统一解析 API。
- 支持 `jdbc` 和 `dm-provider` features。
- 提供跨行日志合并、编码识别、过滤器和完整链路测试。
- 提供 examples、Criterion benchmark 和 90% 覆盖率门禁。
