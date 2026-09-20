# dm-database-driver-log

达梦（DM）驱动日志解析库。这个 crate 只提供 Rust 依赖库 API，不包含命令行程序、Parquet 导出或数据库连接功能。

默认启用 `jdbc` feature。需要解析 `DmProvider_*.log` 格式时启用 `dm-provider` feature：

```toml
[dependencies]
dm-database-driver-log = { version = "0.1", features = ["dm-provider"] }
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

## 扩展其他驱动日志

文件读取、编码处理、单行/跨行 framing、错误上下文和通用过滤器位于统一引擎中。新增驱动时实现 `LogFormat` 和 `LogRecord` 即可复用：

```rust,ignore
use dm_database_driver_log::advanced::{LogFormat, LogParserBuilder, LogRecord, RecordFraming};

struct OtherDriverFormat;
struct OtherDriverEvent {
    method: String,
    used_time_ms: Option<f64>,
    exec_id: Option<i64>,
}

impl LogRecord for OtherDriverEvent {
    fn method(&self) -> &str { &self.method }
    fn category(&self) -> &str { "other-driver" }
    fn used_time_ms(&self) -> Option<f64> { self.used_time_ms }
    fn exec_id(&self) -> Option<i64> { self.exec_id }
}

// 为 OtherDriverFormat 实现 LogFormat 后即可使用：
// advanced::LogParserBuilder::<OtherDriverFormat>::new(path).build()?.iter()?;
```

`RecordFraming::Line` 适合一行一条记录；`RecordFraming::HeaderDelimited` 适合 SQL 或调用栈跨行的日志。
高级通用引擎位于 `advanced` 命名空间，内置 JDBC/DM Provider 的日常调用不需要接触它。

新增一种驱动日志时按以下顺序处理：

1. 在 `src/formats/<driver>.rs` 定义事件和格式解析器。
2. 为事件实现 `LogRecord`，为格式实现 `LogFormat`，选择合适的 `RecordFraming`。
3. 在 `src/formats/mod.rs` 增加 feature 条件模块。
4. 在 `Cargo.toml` 增加该驱动 feature，并在 `lib.rs` 暴露格式事件和兼容 Builder 名称。
5. 在 `tests/integration_test.rs` 增加从文件、跨行边界、错误、过滤和汇总结果的完整链路测试。

## 使用方式

加入依赖：

```toml
[dependencies]
dm-database-driver-log = "0.1"
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

- `LogParserBuilder`：自动识别 JDBC/DM Provider 并构建文件解析器。
- `LogParserBuilder::encoding_hint()`：选择 `Auto`、`Utf8` 或 `Gb18030`。
- `LogParser::format()` / `LogParser::iter()`：查看格式并返回统一流式迭代器。
- `LogIterator::filter_by_method()`：按方法筛选。
- `LogIterator::filter_by_category()`：按事件分类筛选。
- `LogIterator::filter_by_used_time()`：按驱动耗时筛选。
- `LogIterator::filter_by_exec_id()`：按执行编号筛选。
- `parse_line()` / `parse_bytes()` / `parse_bytes_with_encoding()`：解析单条日志。
- `LogEvent::as_jdbc()` / `LogEvent::as_dm_provider()`：访问格式专属字段。
- `advanced::{LogFormat, LogRecord, LogParserBuilder<F>}`：扩展其他驱动日志格式。

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

- `CI`：检查格式、全部 feature 组合测试、Clippy、文档、examples/benchmark
  编译、crate 打包和 90% 覆盖率门禁。
- `Release to crates.io`：推送 `v*` tag 时校验版本、测试、构建并使用
  `CRATES_IO_TOKEN` 发布。
- `Update Benchmark Baseline`：在 Actions 页面手动触发，运行 Criterion 并更新
  `benchmarks/baseline.json`。

本地可以用下面的脚本检查 Criterion 结果是否超过 5% 回归阈值：

```bash
bash scripts/check-regression.sh
```
