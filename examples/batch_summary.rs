//! 将 JDBC 驱动日志汇总为适合脚本消费的统计输出。
//!
//! 用法：`cargo run --example batch_summary -- <path-to-jdbc-log>`

use dm_database_driver_log::LogParserBuilder;
use std::collections::BTreeMap;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: batch_summary <path-to-jdbc-log>");
        std::process::exit(1);
    });

    let parser = LogParserBuilder::new(&path).build()?;
    let mut categories = BTreeMap::<String, usize>::new();
    let mut parsed = 0usize;
    let mut errors = 0usize;
    let mut total_ms = 0.0;

    for result in parser.iter()? {
        match result {
            Ok(event) => {
                parsed += 1;
                *categories.entry(event.category().to_owned()).or_default() += 1;
                total_ms += event.used_time_ms().unwrap_or_default();
            }
            Err(_) => errors += 1,
        }
    }

    println!("records={parsed}");
    println!("errors={errors}");
    println!("total_used_time_ms={total_ms:.6}");
    for (category, count) in categories {
        println!("category.{category}={count}");
    }

    Ok(())
}
