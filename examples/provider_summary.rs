//! 汇总 `DmProvider_*.log` 的记录分类和耗时。
//!
//! 用法：`cargo run --features dm-provider --example provider_summary -- <path>`

use dm_database_driver_log::LogParserBuilder;
use std::collections::BTreeMap;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: provider_summary <path-to-dm-provider-log>");
        std::process::exit(1);
    });

    let parser = LogParserBuilder::new(&path).build()?;
    let mut categories = BTreeMap::<String, usize>::new();
    let mut parsed = 0usize;
    let mut errors = 0usize;
    let mut total_ms = 0.0;
    let mut sql_records = 0usize;

    for result in parser.iter()? {
        match result {
            Ok(event) => {
                parsed += 1;
                *categories.entry(event.category().to_owned()).or_default() += 1;
                total_ms += event.used_time_ms().unwrap_or_default();
                sql_records += usize::from(
                    event
                        .as_dm_provider()
                        .is_some_and(|provider| provider.sql.is_some()),
                );
            }
            Err(_) => errors += 1,
        }
    }

    println!("records={parsed}");
    println!("errors={errors}");
    println!("sql_records={sql_records}");
    println!("total_used_time_ms={total_ms:.6}");
    for (category, count) in categories {
        println!("category.{category}={count}");
    }

    Ok(())
}
