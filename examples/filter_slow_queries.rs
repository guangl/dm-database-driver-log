//! 演示按耗时筛选 JDBC 驱动日志。
//!
//! 用法：`cargo run --example filter_slow_queries -- <path-to-jdbc-log> [min-ms]`

use dm_database_driver_log::LogParserBuilder;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: filter_slow_queries <path-to-jdbc-log> [min-ms]");
        std::process::exit(1);
    });
    let min_ms = env::args()
        .nth(2)
        .map(|value| value.parse::<f64>())
        .transpose()?
        .unwrap_or(100.0);

    let parser = LogParserBuilder::new(&path).build()?;
    for result in parser.iter()?.filter_by_used_time(min_ms) {
        let event = result?;
        println!(
            "line={} method={} used_time_ms={:?} exec_id={:?}",
            event.line_number(),
            event.method(),
            event.used_time_ms(),
            event.exec_id()
        );
    }

    Ok(())
}
