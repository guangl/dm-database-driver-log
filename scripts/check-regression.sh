#!/usr/bin/env bash

set -euo pipefail

baseline_path="${1:-benchmarks/baseline.json}"
threshold_percent="${BENCHMARK_REGRESSION_THRESHOLD:-5}"

python3 - "$baseline_path" "$threshold_percent" <<'PY'
import json
import sys
from pathlib import Path

baseline_path = Path(sys.argv[1])
threshold = float(sys.argv[2])

if not baseline_path.is_file():
    raise SystemExit(f"ERROR: baseline file not found: {baseline_path}")

with baseline_path.open() as stream:
    baselines = json.load(stream)

failed = False
for key, baseline in sorted(baselines.items()):
    group, benchmark = key.split("/", 1)
    estimates_path = Path("target/criterion") / group / benchmark / "new" / "estimates.json"
    if not estimates_path.is_file():
        print(f"WARNING: estimates not found for {key}, skipping")
        continue

    with estimates_path.open() as stream:
        current = json.load(stream)["mean"]["point_estimate"]
    regression = (current - baseline) / baseline * 100
    print(
        f"{key}: baseline={baseline:.0f} ns current={current:.0f} ns "
        f"regression={regression:.1f}% threshold={threshold:.1f}%"
    )
    if regression > threshold:
        failed = True

if failed:
    raise SystemExit("Benchmark regression exceeds the configured threshold")

print("All available benchmarks are within the regression threshold.")
PY
