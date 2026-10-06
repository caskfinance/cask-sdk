"""Run from crates/cask-sdk-python: .venv/bin/python ../../benches/python_bench.py

Requires `maturin develop --release` for representative numbers.
"""
import socket
import subprocess
import time
from pathlib import Path

from cask import Client, Config

ROOT = Path(__file__).resolve().parents[1]
SIZES = [(1_000, 20, 10), (100_000, 500, 50)]
TOTAL = 500_000
SAMPLED = 200_000


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def start_server(customers, pvs, features):
    subprocess.run(["cargo", "build", "-q", "--release", "-p", "cask-mock-server"], cwd=ROOT, check=True)
    port = free_port()
    proc = subprocess.Popen(
        [str(ROOT / "target/release/cask-mock-server"), "--addr", f"127.0.0.1:{port}",
         "--synthetic", f"{customers},{pvs},{features}"],
        stderr=subprocess.DEVNULL,
    )
    return proc, f"http://127.0.0.1:{port}"


def bench(customers, pvs, features):
    print(f"\n== {customers} customers, {pvs} product versions, {features} features ==")
    proc, url = start_server(customers, pvs, features)
    try:
        time.sleep(1.5 if customers > 10_000 else 0.5)
        t = time.perf_counter()
        cask = Client(Config("test-key", base_url=url))
        cask.wait_until_ready(60)
        print(f"download+parse+index: {(time.perf_counter() - t) * 1e3:.1f} ms")

        ids = [f"customer_{i:032X}" for i in range(customers)]
        names = [f"feature_{i}" for i in range(features)]
        fns = [cask.check_bool, cask.check_numeric, cask.check_enum]

        def run(i):
            f = i % features
            return fns[f % 3](ids[(i * 2654435761) % customers], names[f])

        t = time.perf_counter_ns()
        for i in range(TOTAL):
            run(i)
        mean = (time.perf_counter_ns() - t) / TOTAL

        samples = []
        for i in range(SAMPLED):
            t = time.perf_counter_ns()
            run(i)
            samples.append(time.perf_counter_ns() - t)
        samples.sort()
        pct = lambda p: samples[int((len(samples) - 1) * p)]
        print(f"check: mean {mean:.0f} ns | p50 {pct(0.5)} ns | p99 {pct(0.99)} ns | "
              f"p99.9 {pct(0.999)} ns | max {samples[-1]} ns")
        cask.close()
    finally:
        proc.terminate()
        proc.wait()


for size in SIZES:
    bench(*size)
