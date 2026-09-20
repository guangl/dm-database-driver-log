use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TARGET_BYTES: usize = 5 * 1024 * 1024;

#[cfg(feature = "jdbc")]
const JDBC_SINGLE_LINE: &[u8] = b"[INFO  - 2026-09-16 17:45:19.763] tid:119 - [benchmark] { conn-3, pstmt-854 } executeQuery(): rs-2216; [USED TIME]: 8.5ms; [EXEC_ID]: 19010657;\n";

#[cfg(feature = "jdbc")]
const JDBC_MULTILINE_LINE: &[u8] = b"[SQL   - 2026-09-16 17:45:19.763] tid:119 - [benchmark] { conn-3, pstmt-854 } prepareStatement(String): pstmt-854; [PARAMS]: \"SELECT id,\n    name\nFROM benchmark_table\nWHERE id = 12345\"; [USED TIME]: 8.5ms; [EXEC_ID]: 19010657;\n";

#[cfg(feature = "dm-provider")]
const PROVIDER_SINGLE_LINE: &[u8] = b"[INFO - 2026-09-12 08:55:30.193] tid:68 (benchmark) { conn-2095 (sessId:281421579449976), command-4579 } ExecuteDbDataReader(CommandBehavior) [RETURN]: dateReader-2971; [PARAMS]: Default; [SQL]: SELECT * FROM benchmark_table WHERE id = 12345 [USED TIME]: 2ms; [EXEC_ID]: 908131301;\n";

#[cfg(feature = "dm-provider")]
const PROVIDER_MULTILINE_RECORD: &[u8] = b"[SQL - 2026-09-12 08:55:30.193] tid:68 (benchmark) { conn-2095 (sessId:281421579449976), command-4579 } ExecuteDbDataReader(CommandBehavior) [RETURN]: dateReader-2971; [PARAMS]: Default; [SQL]: SELECT id,\n    name\nFROM benchmark_table\nWHERE id = 12345 [USED TIME]: 2ms; [EXEC_ID]: 908131301;\n";

struct TempLog {
    path: PathBuf,
}

impl TempLog {
    fn from_repeating(name: &str, target_bytes: usize, records: &[&[u8]]) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "dm-driver-benchmark-{name}-{}-{nonce}.log",
            std::process::id()
        ));
        let mut data = Vec::with_capacity(target_bytes + records[0].len());
        let mut index = 0usize;
        while data.len() < target_bytes {
            data.extend_from_slice(records[index % records.len()]);
            index += 1;
        }
        std::fs::write(&path, data).expect("write benchmark log");
        Self { path }
    }
}

impl Drop for TempLog {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(feature = "jdbc")]
fn count_jdbc_records(path: &Path) -> usize {
    let parser = dm_database_driver_log::DriverLogParserBuilder::new(path)
        .build()
        .expect("open JDBC benchmark log");
    parser.iter().expect("iterate JDBC benchmark log").count()
}

#[cfg(feature = "dm-provider")]
fn count_dm_provider_records(path: &Path) -> usize {
    let parser = dm_database_driver_log::DmProviderLogParserBuilder::new(path)
        .build()
        .expect("open Provider benchmark log");
    parser
        .iter()
        .expect("iterate Provider benchmark log")
        .count()
}

#[cfg(feature = "jdbc")]
fn benchmark_jdbc(c: &mut Criterion) {
    use dm_database_driver_log::DriverLogParserBuilder;

    let mut group = c.benchmark_group("jdbc_driver_parser");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(10));
    group.warm_up_time(Duration::from_secs(2));

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let real_path = root.join("dm_jdbc_2026_09_16_17_39_37_14 (2)(勿删).log");
    if real_path.exists() {
        group.bench_function("parse_real_jdbc_log", |b| {
            b.iter(|| {
                let parser = DriverLogParserBuilder::new(&real_path).build().unwrap();
                black_box(parser.iter().unwrap().count());
            });
        });
    } else {
        eprintln!("Note: real JDBC log not found, using synthetic data");
    }

    let single = TempLog::from_repeating("jdbc-single", TARGET_BYTES, &[JDBC_SINGLE_LINE]);
    group.throughput(Throughput::Bytes(TARGET_BYTES as u64));
    group.bench_function("parse_jdbc_5mb", |b| {
        b.iter(|| {
            let parser = DriverLogParserBuilder::new(&single.path).build().unwrap();
            black_box(parser.iter().unwrap().count());
        });
    });

    let single_count = count_jdbc_records(&single.path) as u64;
    group.throughput(Throughput::Elements(single_count));
    group.bench_function("parse_jdbc_5mb_records_per_second", |b| {
        b.iter(|| {
            let parser = DriverLogParserBuilder::new(&single.path).build().unwrap();
            black_box(parser.iter().unwrap().count());
        });
    });

    let multiline = TempLog::from_repeating(
        "jdbc-multiline",
        TARGET_BYTES,
        &[JDBC_SINGLE_LINE, JDBC_MULTILINE_LINE],
    );
    group.throughput(Throughput::Bytes(TARGET_BYTES as u64));
    group.bench_function("parse_jdbc_5mb_mixed_lines", |b| {
        b.iter(|| {
            let parser = DriverLogParserBuilder::new(&multiline.path)
                .build()
                .unwrap();
            black_box(parser.iter().unwrap().count());
        });
    });

    group.finish();
}

#[cfg(feature = "dm-provider")]
fn benchmark_dm_provider(c: &mut Criterion) {
    use dm_database_driver_log::DmProviderLogParserBuilder;

    let mut group = c.benchmark_group("dm_provider_parser");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(10));
    group.warm_up_time(Duration::from_secs(2));

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let real_path = root.join("DmProvider_2026_09_12_08_37_44.697.log");
    if real_path.exists() {
        group.bench_function("parse_real_dm_provider_log", |b| {
            b.iter(|| {
                let parser = DmProviderLogParserBuilder::new(&real_path).build().unwrap();
                black_box(parser.iter().unwrap().count());
            });
        });
    } else {
        eprintln!("Note: real DM Provider log not found, using synthetic data");
    }

    let single =
        TempLog::from_repeating("dm-provider-single", TARGET_BYTES, &[PROVIDER_SINGLE_LINE]);
    group.throughput(Throughput::Bytes(TARGET_BYTES as u64));
    group.bench_function("parse_dm_provider_5mb", |b| {
        b.iter(|| {
            let parser = DmProviderLogParserBuilder::new(&single.path)
                .build()
                .unwrap();
            black_box(parser.iter().unwrap().count());
        });
    });

    let record_count = count_dm_provider_records(&single.path) as u64;
    group.throughput(Throughput::Elements(record_count));
    group.bench_function("parse_dm_provider_5mb_records_per_second", |b| {
        b.iter(|| {
            let parser = DmProviderLogParserBuilder::new(&single.path)
                .build()
                .unwrap();
            black_box(parser.iter().unwrap().count());
        });
    });

    let multiline = TempLog::from_repeating(
        "dm-provider-multiline",
        TARGET_BYTES,
        &[PROVIDER_SINGLE_LINE, PROVIDER_MULTILINE_RECORD],
    );
    group.throughput(Throughput::Bytes(TARGET_BYTES as u64));
    group.bench_function("parse_dm_provider_5mb_mixed_records", |b| {
        b.iter(|| {
            let parser = DmProviderLogParserBuilder::new(&multiline.path)
                .build()
                .unwrap();
            black_box(parser.iter().unwrap().count());
        });
    });

    group.finish();
}

fn benchmark_parser(c: &mut Criterion) {
    #[cfg(feature = "jdbc")]
    benchmark_jdbc(c);
    #[cfg(feature = "dm-provider")]
    benchmark_dm_provider(c);
}

criterion_group!(benches, benchmark_parser);
criterion_main!(benches);
