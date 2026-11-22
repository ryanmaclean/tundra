#!/bin/bash
# dd_profiling_demo.sh - Demonstrate dd-based profiling for Rust apps

echo "=== dd Profiling Demo for Rust Apps ==="

# 1. Disk I/O Profiling with dd
echo "1. Disk I/O Benchmarking:"
dd if=/dev/zero of=/tmp/test_file bs=1M count=100 2>&1 | grep -E "(copied|MB/s)"

# 2. Memory-mapped file profiling
echo "2. Memory-mapped file I/O:"
dd if=/dev/zero of=/tmp/mmap_test bs=4k count=2560 2>&1 | grep -E "(copied|MB/s)"

# 3. Rust app disk usage profiling
echo "3. Rust app disk usage:"
cargo build --release --bin at-daemon
du -sh target/release/at-daemon

# 4. Profile Rust app startup time with dd
echo "4. Rust app startup time profiling:"
time (cargo run --release --bin at-daemon --help > /dev/null 2>&1)

# 5. Memory profiling with dd (core dump analysis)
echo "5. Create memory dump for analysis:"
# Note: This requires the app to be running
# gcore -o /tmp/at-daemon-core $(pgrep at-daemon)

echo "=== dd vs Modern Profiling ==="
echo "dd: Basic disk I/O benchmarking"
echo "cargo-flamegraph: CPU profiling with call graphs"
echo "cargo-profiler: Memory profiling"
echo "perf: System-wide profiling"

# Cleanup
rm -f /tmp/test_file /tmp/mmap_test
