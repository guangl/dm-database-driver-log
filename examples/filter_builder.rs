//! 演示使用通用过滤器筛选 JDBC 驱动日志。
//!
//! 用法：`cargo run --example filter_builder -- <path-to-jdbc-log>`

use dm_database_driver_log::LogParserBuilder;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: filter_builder <path-to-jdbc-log>");
        std::process::exit(1);
    });

    let parser = LogParserBuilder::new(&path).build()?;
    let mut count = 0usize;

    for result in parser.iter()?.filter_by_category("execute") {
        let event = result?;
        println!(
            "line={} method={} exec_id={:?} used_time_ms={:?}",
            event.line_number(),
            event.method(),
            event.exec_id(),
            event.used_time_ms()
        );
        count += 1;
    }

    println!("matched {count} execute records");
    Ok(())
}
