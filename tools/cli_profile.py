#!/usr/bin/env python3
"""
Profile and compare `mcap` vs `mcapable` CLI runtimes on a given input file.

Examples:
  python3 tools/cli_profile.py HKisland01_0.mcap
  python3 tools/cli_profile.py HKisland01_0-trunc.mcap --only info,doctor,du
  python3 tools/cli_profile.py HKisland01_0-trunc.mcap --include-write --only recover
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import time
import uuid
from dataclasses import dataclass
from typing import Iterable, Optional


ANSI_RED = "\x1b[31m"
ANSI_GREEN = "\x1b[32m"
ANSI_YELLOW = "\x1b[33m"
ANSI_RESET = "\x1b[0m"


def is_tty() -> bool:
    try:
        return sys.stdout.isatty()
    except Exception:
        return False


def colorize(s: str, color: str, *, enable: bool) -> str:
    if not enable:
        return s
    return f"{color}{s}{ANSI_RESET}"


def fmt_seconds(seconds: float) -> str:
    if seconds < 1e-3:
        return f"{seconds * 1e6:.2f} µs"
    if seconds < 1.0:
        return f"{seconds * 1e3:.2f} ms"
    return f"{seconds:.3f} s"


def fmt_pct(delta: Optional[float]) -> str:
    if delta is None:
        return "n/a"
    return f"{delta * 100:+.2f}%"


def which_or_die(name_or_path: str) -> str:
    if os.path.sep in name_or_path or name_or_path.startswith("."):
        if os.path.exists(name_or_path):
            return name_or_path
        raise SystemExit(f"binary not found: {name_or_path}")
    resolved = shutil.which(name_or_path)
    if not resolved:
        raise SystemExit(f"binary not found in PATH: {name_or_path}")
    return resolved


@dataclass(frozen=True)
class Case:
    key: str
    label: str
    mcap_args: list[str]
    mcapable_args: list[str]
    needs_output: bool = False


@dataclass(frozen=True)
class ResultRow:
    key: str
    label: str
    mcapable_s: Optional[float]
    mcap_s: Optional[float]
    delta: Optional[float]  # (mcapable/mcap - 1)
    mcapable_rc: Optional[int]
    mcap_rc: Optional[int]


def run_once(argv: list[str], *, quiet: bool) -> tuple[float, int]:
    stdout = subprocess.DEVNULL if quiet else None
    stderr = subprocess.DEVNULL if quiet else None
    t0 = time.perf_counter()
    proc = subprocess.run(argv, stdout=stdout, stderr=stderr)
    t1 = time.perf_counter()
    return (t1 - t0, proc.returncode)


def measure(argv: list[str], *, iters: int, quiet: bool) -> tuple[float, int]:
    samples: list[float] = []
    last_rc = 0
    for _ in range(iters):
        dt, rc = run_once(argv, quiet=quiet)
        samples.append(dt)
        last_rc = rc
    return (statistics.median(samples), last_rc)


def make_temp_output(output_dir: str, suffix: str) -> str:
    os.makedirs(output_dir, exist_ok=True)
    return os.path.join(output_dir, f"cli_profile_{uuid.uuid4().hex}{suffix}")


def build_cases(input_path: str, *, include_write: bool, output_dir: str) -> list[Case]:
    cases: list[Case] = []

    # Read-only / low output
    cases.extend(
        [
            Case("version", "version", ["version"], ["version"]),
            Case("info", "info", ["info", input_path], ["info", input_path]),
            Case("doctor", "doctor", ["doctor", input_path], ["doctor", input_path]),
            Case("du", "du", ["du", input_path], ["du", input_path]),
            Case("list_channels", "list channels", ["list", "channels", input_path], ["list", "channels", input_path]),
            Case("list_schemas", "list schemas", ["list", "schemas", input_path], ["list", "schemas", input_path]),
            Case("list_chunks", "list chunks", ["list", "chunks", input_path], ["list", "chunks", input_path]),
            Case("list_metadata", "list metadata", ["list", "metadata", input_path], ["list", "metadata", input_path]),
            Case("list_attachments", "list attachments", ["list", "attachments", input_path], ["list", "attachments", input_path]),
        ]
    )

    if include_write:
        out_recover_mcap = make_temp_output(output_dir, ".mcap")
        out_recover_mcapable = make_temp_output(output_dir, ".mcap")
        cases.append(
            Case(
                "recover",
                "recover (default)",
                ["recover", input_path, "-o", out_recover_mcap],
                ["recover", input_path, "-o", out_recover_mcapable],
                needs_output=True,
            )
        )

    return cases


def print_table(rows: list[ResultRow], *, color: bool) -> None:
    # Column widths
    w_label = max([len(r.label) for r in rows] + [7])
    header = f"{'test':<{w_label}}  {'mcapable':>12}  {'mcap':>12}  {'diff':>9}"
    print(header)
    print("-" * len(header))

    for r in rows:
        a = "n/a" if r.mcapable_s is None else fmt_seconds(r.mcapable_s)
        b = "n/a" if r.mcap_s is None else fmt_seconds(r.mcap_s)
        d = fmt_pct(r.delta)
        if r.delta is not None:
            if r.delta < 0:
                d = colorize(d, ANSI_GREEN, enable=color)
            elif r.delta > 0:
                d = colorize(d, ANSI_RED, enable=color)
            else:
                d = colorize(d, ANSI_YELLOW, enable=color)
        print(f"{r.label:<{w_label}}  {a:>12}  {b:>12}  {d:>9}")


def main() -> int:
    ap = argparse.ArgumentParser(description="Compare mcap vs mcapable CLI runtimes.")
    ap.add_argument("input", help="Input .mcap file path")
    ap.add_argument("--mcap", default="mcap", help="mcap CLI binary (default: mcap)")
    ap.add_argument(
        "--mcapable",
        default="./target/release/mcapable" if os.path.exists("./target/release/mcapable") else "mcapable",
        help="mcapable CLI binary (default: ./target/release/mcapable if present)",
    )
    ap.add_argument("--iterations", type=int, default=1, help="iterations per case (median)")
    ap.add_argument("--include-write", action="store_true", help="include write/output commands (slow, big)")
    ap.add_argument("--output-dir", default=".", help="directory for temporary outputs for write cases")
    ap.add_argument("--keep-outputs", action="store_true", help="do not delete temporary output files")
    ap.add_argument(
        "--only",
        default="",
        help="comma-separated case keys to run (e.g. info,doctor,recover)",
    )
    ap.add_argument("--json", dest="json_path", default="", help="write results JSON to this path")
    ap.add_argument("--no-color", action="store_true", help="disable ANSI colors")
    ap.add_argument("--verbose", action="store_true", help="show stdout/stderr from commands")
    args = ap.parse_args()

    input_path = args.input
    if not os.path.exists(input_path):
        raise SystemExit(f"input file not found: {input_path}")

    mcap_bin = which_or_die(args.mcap)
    mcapable_bin = which_or_die(args.mcapable)

    cases = build_cases(input_path, include_write=args.include_write, output_dir=args.output_dir)
    only = {s.strip() for s in args.only.split(",") if s.strip()}
    if only:
        cases = [c for c in cases if c.key in only]
        missing = sorted(only - {c.key for c in cases})
        if missing:
            raise SystemExit(f"unknown case keys: {', '.join(missing)}")

    rows: list[ResultRow] = []
    quiet = not args.verbose

    for c in cases:
        mcapable_cmd = [mcapable_bin, *c.mcapable_args]
        mcap_cmd = [mcap_bin, *c.mcap_args]

        mcapable_s, mcapable_rc = measure(mcapable_cmd, iters=args.iterations, quiet=quiet)
        mcap_s, mcap_rc = measure(mcap_cmd, iters=args.iterations, quiet=quiet)

        delta = None
        if mcap_s > 0:
            delta = (mcapable_s / mcap_s) - 1.0

        rows.append(
            ResultRow(
                key=c.key,
                label=c.label,
                mcapable_s=mcapable_s,
                mcap_s=mcap_s,
                delta=delta,
                mcapable_rc=mcapable_rc,
                mcap_rc=mcap_rc,
            )
        )

        if c.needs_output and not args.keep_outputs:
            # Best-effort cleanup: outputs are embedded in args in fixed positions.
            for argv in (mcapable_cmd, mcap_cmd):
                if "-o" in argv:
                    out_path = argv[argv.index("-o") + 1]
                    try:
                        os.remove(out_path)
                    except FileNotFoundError:
                        pass

    rows.sort(key=lambda r: r.key)

    color = (not args.no_color) and is_tty()
    print_table(rows, color=color)

    if args.json_path:
        payload = [
            {
                "key": r.key,
                "label": r.label,
                "mcapable_seconds": r.mcapable_s,
                "mcap_seconds": r.mcap_s,
                "delta": r.delta,
                "mcapable_rc": r.mcapable_rc,
                "mcap_rc": r.mcap_rc,
            }
            for r in rows
        ]
        with open(args.json_path, "w", encoding="utf-8") as f:
            json.dump(payload, f, indent=2, sort_keys=True)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
