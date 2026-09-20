# dm-database-driver-log

[![CI](https://github.com/guangl/dm-database-driver-log/actions/workflows/ci.yml/badge.svg)](https://github.com/guangl/dm-database-driver-log/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/dm-database-driver-log.svg)](https://crates.io/crates/dm-database-driver-log)
[![Documentation](https://docs.rs/dm-database-driver-log/badge.svg)](https://docs.rs/dm-database-driver-log)
[![Rust Version](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/crates/l/dm-database-driver-log.svg)](https://github.com/guangl/dm-database-driver-log/blob/main/LICENSE)

达梦（DM）驱动日志解析库。这个 crate 只提供 Rust 依赖库 API，不包含命令行程序、Parquet 导出或数据库连接功能。

版本记录见 [CHANGELOG.md](CHANGELOG.md)。

要求 Rust 1.85 或更高版本（Rust 2024 edition）。库、examples 和 benchmark 均遵循该
最低版本，CI 也会检查 MSRV 兼容性。

默认启用 `jdbc` feature。需要解析 `DmProvider_*.log` 格式时启用 `dm-provider` feature：

```toml
[dependencies]
dm-database-driver-log = { version = "0.2", features = ["dm-provider"] }
```

## 支持的日志格式

### JDBC（`jdbc`）

```text
[INFO  - 2026-09-16 17:45:19.763] tid:119 - [worker] { conn-3, pstmt-854, rs-2216 } getString(String): "value"; [PARAMS]: "name"; [USED TIME]: 0.5ms;
[INFO  - 2026-09-16 17:45:19.763] tid:119 - [worker] { conn-3, pstmt-854 } executeQuery(): rs-2216; [USED TIME]: 8.5ms; [EXEC_ID]: 19010657;
[DEBUG - 2026-09-16 17:45:19.763] tid:119 - [worker] { conn-3 } access(); CMD_EXECUTE2
```

### DM Provider（`dm-provider`）

```text
[INFO - 2026-09-12 08:37:44.696] tid:34 (IsBackground-True) { B@16900fb } access Cmd:4(); [USED TIME]: 0ns;
[SQL - 2026-09-12 08:55:30.193] tid:68 (IsBackground-True) { conn-2095 (sessId:281421579449976), command-4579 } ExecuteDbDataReader(CommandBehavior) [SQL]: SELECT ... [USED TIME]: 2ms; [EXEC_ID]: 908131301;
```

Provider 日志中的 SQL 可以跨物理行；使用统一的 `LogParserBuilder` 时会自动识别格式，
并将跨行内容合并到同一个 `LogEvent::DmProvider` 事件。`used_time_ms` 统一换算成毫秒，
原始值保存在 `used_time_text`。

## 统一 API

JDBC 和 DM Provider 共用同一套入口，解析器会根据第一条记录的日志头自动识别格式：

```rust,no_run
use dm_database_driver_log::{LogEvent, LogParserBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let parser = LogParserBuilder::new("driver.log").build()?;

    for result in parser.iter()? {
        let event = result?;
        println!(
            "format={:?} line={} method={} category={} used_time_ms={:?}",
            event.format(),
            event.line_number(),
            event.method(),
            event.category(),
            event.used_time_ms(),
        );

        match event {
            #[cfg(feature = "jdbc")]
            LogEvent::Jdbc(jdbc) => println!("jdbc thread={}", jdbc.thread),
            #[cfg(feature = "dm-provider")]
            LogEvent::DmProvider(provider) => println!("provider ids={}", provider.ids),
        }
    }
    Ok(())
}
```

单条记录也使用同一套入口：`parse_line()`、`parse_bytes()` 和
`parse_bytes_with_encoding()` 返回 `LogEvent`。需要访问格式专属字段时使用
`event.as_jdbc()` 或 `event.as_dm_provider()`。

## 其他驱动日志

本 crate 的公开 API 只覆盖 JDBC 和 DM Provider。需要支持其他驱动时，请 fork 本仓库，
在 `src/formats/` 中增加格式模块，并同步修改 feature、统一事件枚举、格式识别和完整链路测试。

## 使用方式

加入依赖：

```toml
[dependencies]
dm-database-driver-log = "0.2"
```

逐行流式解析文件：

```rust,no_run
use dm_database_driver_log::LogParserBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let parser = LogParserBuilder::new("driver.log").build()?;

    for result in parser.iter()? {
        let event = result?;
        println!(
            "line={} method={} used_time_ms={:?}",
            event.line_number(), event.method(), event.used_time_ms()
        );
    }
    Ok(())
}
```

也可以链式筛选：

```rust,no_run
use dm_database_driver_log::LogParserBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let parser = LogParserBuilder::new("driver.log").build()?;
    let slow_queries = parser
        .iter()?
        .filter_by_method("executeQuery")
        .filter_by_used_time(10.0);

    for result in slow_queries {
        let event = result?;
        println!("exec_id={:?} used_time_ms={:?}", event.exec_id(), event.used_time_ms());
    }
    Ok(())
}
```

单行解析返回可独立保存的统一 `LogEvent`；格式专属字段通过 `as_jdbc()` 或
`as_dm_provider()` 借用访问。

文件迭代器会跳过空行，并保留物理行号。格式错误会包含行号、字节偏移和原文；可以使用 `skip_errors()` 忽略错误，或保留错误结果进行诊断。

## API

### 内置驱动日志

日常使用只需要统一 API：

- `LogParserBuilder`：自动识别 JDBC/DM Provider 并构建文件解析器。
- `LogParserBuilder::encoding_hint()`：选择 `Auto`、`Utf8` 或 `Gb18030`。
- `LogParser::format()` / `LogParser::iter()`：查看格式并创建统一流式迭代器。
- `LogIterator::filter_by_method()` / `filter_by_category()`：按方法或分类筛选。
- `LogIterator::filter_by_used_time()` / `filter_by_exec_id()`：按耗时或执行编号筛选。
- `LogIterator::skip_errors()`：忽略格式错误，只保留成功解析的事件。
- `LogEvent`：通过 `format()`、`method()`、`category()`、`used_time_ms()`、`exec_id()` 等方法访问公共字段。
- `LogEvent::as_jdbc()` / `LogEvent::as_dm_provider()`：访问格式专属字段。
- `parse_line()` / `parse_bytes()` / `parse_bytes_with_encoding()`：解析单条日志。
- `FileEncodingHint` / `ParseError`：控制输入编码并处理解析错误。

启用 `jdbc` feature 后可使用 `JdbcEvent`，启用 `dm-provider` feature 后可使用
`DmProviderEvent` 访问对应格式的专属结构。默认 feature 是 `jdbc`。

## 测试覆盖率

项目将行覆盖率 90% 作为最低门禁。安装 `cargo-llvm-cov` 后执行：

```bash
sh scripts/coverage.sh
```

脚本会运行全部测试，并在行覆盖率低于 90% 时返回失败。

`tests/integration_test.rs` 还提供完整链路测试：从临时日志文件开始，经过
Builder、流式 Iterator、跨行记录合并、错误诊断、过滤器和结果汇总，分别覆盖
JDBC 与 `dm-provider` feature。

## Examples 与 benchmark

examples 是独立的演示程序，不改变本 crate 的依赖库定位：

```bash
cargo run --example batch_summary -- path/to/dm-jdbc.log
cargo run --example filter_builder -- path/to/dm-jdbc.log
cargo run --example filter_slow_queries -- path/to/dm-jdbc.log 100
cargo run --features dm-provider --example provider_summary -- path/to/DmProvider.log
```

`perf_full` 会生成合成 JDBC 日志并输出迭代吞吐；默认约 50 MiB、20 次，
可用环境变量缩短本地试跑：

```bash
PERF_SIZE_MB=5 PERF_ITERS=3 cargo run --release --example perf_full
```

Criterion benchmark 会在仓库中的真实日志存在时优先测量真实文件，同时始终
覆盖合成的 5 MiB 单行/混合记录；启用全部内置格式可运行：

```bash
cargo bench --all-features --bench driver_benchmark
```

其中 JDBC 和 `dm-provider` 的多行样本分别验证对应 framing 的文件流式解析。

## GitHub Actions

- `CI`：在面向 `main` 的 Pull Request 上检查格式、全部 feature 组合测试、Clippy、
  文档、examples/benchmark 编译、crate 打包和 90% 覆盖率门禁；合并到 `main` 后不会
  因为 push 事件重复执行同一套 CI。
- `Release to crates.io`：推送 `v*` tag 时校验版本、测试、构建并使用
  `CRATES_IO_TOKEN` 发布。
- `Update Benchmark Baseline`：在 Actions 页面手动触发，运行 Criterion 并更新
  `benchmarks/baseline.json`。

本地可以用下面的脚本检查 Criterion 结果是否超过 5% 回归阈值：

```bash
bash scripts/check-regression.sh
```
