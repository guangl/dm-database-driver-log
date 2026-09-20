//! 对 JDBC 文件迭代器做一次可重复的合成吞吐测试。
//!
//! 默认生成约 50 MiB 日志并执行 20 次；可通过 `PERF_SIZE_MB` 和
//! `PERF_ITERS` 环境变量降低本地试跑成本。

use dm_database_driver_log::DriverLogParserBuilder;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const RECORD: &[u8] = b"[INFO  - 2026-09-16 17:45:19.763] tid:119 - [benchmark] { conn-3, pstmt-854 } executeQuery(): rs-2216; [USED TIME]: 8.5ms; [EXEC_ID]: 19010657;\n";

fn generate_log_data(target_bytes: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(target_bytes + RECORD.len());
    while data.len() < target_bytes {
        data.extend_from_slice(RECORD);
    }
    data
}

fn bench_iter(path: &str, iterations: usize) -> Vec<Duration> {
    let mut durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let parser = DriverLogParserBuilder::new(path)
            .build()
            .expect("open file");
        let start = Instant::now();
        let count = parser
            .iter()
            .expect("iterate file")
            .filter(|r| r.is_ok())
            .count();
        durations.push(start.elapsed());
        std::hint::black_box(count);
    }
    durations
}

fn throughput_mb_s(bytes: usize, duration: Duration) -> f64 {
    bytes as f64 / 1024.0 / 1024.0 / duration.as_secs_f64()
}

fn report(durations: &[Duration], file_bytes: usize) {
    let mut throughputs: Vec<_> = durations
        .iter()
        .map(|duration| throughput_mb_s(file_bytes, *duration))
        .collect();
    throughputs.sort_by(|left, right| left.partial_cmp(right).unwrap());

    let min = throughputs.first().copied().unwrap_or_default();
    let max = throughputs.last().copied().unwrap_or_default();
    let avg = throughputs.iter().sum::<f64>() / throughputs.len().max(1) as f64;
    let median = if throughputs.len().is_multiple_of(2) && !throughputs.is_empty() {
        let middle = throughputs.len() / 2;
        (throughputs[middle - 1] + throughputs[middle]) / 2.0
    } else {
        throughputs
            .get(throughputs.len() / 2)
            .copied()
            .unwrap_or_default()
    };

    println!(
        "throughput: min={min:.1} MB/s median={median:.1} MB/s avg={avg:.1} MB/s max={max:.1} MB/s"
    );
}

fn main() {
    let size_mb = std::env::var("PERF_SIZE_MB")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(50);
    let iterations = std::env::var("PERF_ITERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20);
    let target_bytes = size_mb * 1024 * 1024;

    println!("generating approximately {size_mb} MiB of synthetic JDBC logs");
    let data = generate_log_data(target_bytes);
    let actual_bytes = data.len();
    let record_count = actual_bytes / RECORD.len();
    println!("generated {actual_bytes} bytes ({record_count} records)");

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("dm-driver-perf-{}-{nonce}.log", std::process::id()));
    std::fs::write(&path, data).expect("write temporary log");

    println!("warming up");
    for _ in 0..3 {
        let parser = DriverLogParserBuilder::new(&path)
            .build()
            .expect("open file");
        std::hint::black_box(parser.iter().expect("iterate file").count());
    }

    println!("benchmarking {iterations} iterations");
    let durations = bench_iter(path.to_str().expect("temporary path is UTF-8"), iterations);
    report(&durations, actual_bytes);
    let _ = std::fs::remove_file(path);
}
