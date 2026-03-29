#!/usr/bin/env python3
"""
Summarize Criterion benchmarks comparing `mcapable` vs `mcap_crate`.

Usage:
  python3 tools/bench_summary.py
  python3 tools/bench_summary.py --root target/criterion --filter writer_disk_write_only
"""

from __future__ import annotations

import argparse
import html
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Tuple


ANSI_GREEN = "\x1b[32m"
ANSI_RED = "\x1b[31m"
ANSI_YELLOW = "\x1b[33m"
ANSI_DIM = "\x1b[2m"
ANSI_RESET = "\x1b[0m"


def _is_tty() -> bool:
    try:
        return sys.stdout.isatty()
    except Exception:
        return False


def _human_time(seconds: float) -> str:
    if seconds < 1e-6:
        return f"{seconds * 1e9:.2f} ns"
    if seconds < 1e-3:
        return f"{seconds * 1e6:.2f} µs"
    if seconds < 1:
        return f"{seconds * 1e3:.2f} ms"
    return f"{seconds:.2f} s"


def _human_bytes_per_sec(bps: float) -> str:
    if bps <= 0:
        return "-"
    units = ["B/s", "KiB/s", "MiB/s", "GiB/s", "TiB/s"]
    v = bps
    idx = 0
    while v >= 1024.0 and idx < len(units) - 1:
        v /= 1024.0
        idx += 1
    return f"{v:.2f} {units[idx]}"


def _pct(delta: float) -> str:
    sign = "+" if delta > 0 else ""
    return f"{sign}{delta:.2f}%"


def _read_json(path: Path) -> Dict[str, Any]:
    with path.open("r", encoding="utf-8") as f:
        return json.load(f)


def _f16_to_f64(h: int) -> float:
    sign = (h >> 15) & 1
    exp = (h >> 10) & 0x1F
    frac = h & 0x3FF

    if exp == 0:
        if frac == 0:
            return -0.0 if sign else 0.0
        return ((-1.0) ** sign) * (frac / 2**10) * 2 ** (-14)

    if exp == 0x1F:
        if frac == 0:
            return float("-inf") if sign else float("inf")
        return float("nan")

    return ((-1.0) ** sign) * (1.0 + frac / 2**10) * 2 ** (exp - 15)


def _cbor_make_hashable(x: Any) -> Any:
    if isinstance(x, (str, int, float, type(None), bool, bytes)):
        return x
    if isinstance(x, list):
        return tuple(_cbor_make_hashable(v) for v in x)
    if isinstance(x, dict):
        return (
            "__map__",
            tuple(sorted((_cbor_make_hashable(k), _cbor_make_hashable(v)) for k, v in x.items())),
        )
    return repr(x)


class _CborDecoder:
    def __init__(self, data: bytes):
        self._data = data
        self._i = 0

    def _read(self, n: int) -> bytes:
        b = self._data[self._i : self._i + n]
        self._i += n
        return b

    def _read_uint(self, ai: int) -> int:
        if ai < 24:
            return ai
        if ai == 24:
            return self._read(1)[0]
        if ai == 25:
            return int.from_bytes(self._read(2), "big")
        if ai == 26:
            return int.from_bytes(self._read(4), "big")
        if ai == 27:
            return int.from_bytes(self._read(8), "big")
        raise ValueError("indefinite-length CBOR not supported")

    def decode(self) -> Any:
        import struct

        b = self._read(1)[0]
        major = b >> 5
        ai = b & 0x1F

        if major == 0:
            return self._read_uint(ai)
        if major == 1:
            return -1 - self._read_uint(ai)
        if major == 2:
            return self._read(self._read_uint(ai))
        if major == 3:
            return self._read(self._read_uint(ai)).decode("utf-8")
        if major == 4:
            return [self.decode() for _ in range(self._read_uint(ai))]
        if major == 5:
            out: Dict[Any, Any] = {}
            for _ in range(self._read_uint(ai)):
                k = self.decode()
                v = self.decode()
                out[_cbor_make_hashable(k)] = v
            return out
        if major == 6:
            _ = self._read_uint(ai)  # tag
            return self.decode()
        if major == 7:
            if ai == 20:
                return False
            if ai == 21:
                return True
            if ai == 22 or ai == 23:
                return None
            if ai == 24:
                return self._read(1)[0]
            if ai == 25:
                return _f16_to_f64(int.from_bytes(self._read(2), "big"))
            if ai == 26:
                return struct.unpack(">f", self._read(4))[0]
            if ai == 27:
                return struct.unpack(">d", self._read(8))[0]
            raise ValueError(f"unsupported CBOR simple value: {ai}")

        raise ValueError(f"unsupported CBOR major type: {major}")


def _read_cbor(path: Path) -> Any:
    dec = _CborDecoder(path.read_bytes())
    return dec.decode()


def _get_nested(obj: Dict[str, Any], keys: Iterable[str]) -> Optional[Any]:
    cur: Any = obj
    for k in keys:
        if not isinstance(cur, dict) or k not in cur:
            return None
        cur = cur[k]
    return cur


@dataclass(frozen=True)
class Estimate:
    mean_seconds: float
    throughput_bytes: Optional[int]


def _load_estimate(bench_dir: Path) -> Optional[Estimate]:
    estimates_path = bench_dir / "new" / "estimates.json"
    if not estimates_path.exists():
        return None

    estimates = _read_json(estimates_path)
    mean_ns = _get_nested(estimates, ["mean", "point_estimate"])
    if mean_ns is None:
        mean_ns = _get_nested(estimates, ["median", "point_estimate"])
    if mean_ns is None:
        return None

    throughput_bytes: Optional[int] = None
    for candidate in [bench_dir / "benchmark.json", bench_dir / "new" / "benchmark.json"]:
        if candidate.exists():
            bench_meta = _read_json(candidate)
            thr = bench_meta.get("throughput")
            if isinstance(thr, dict):
                # Criterion may serialize as {"Bytes": N} or similar.
                for k in ("Bytes", "BytesDecimal"):
                    v = thr.get(k)
                    if isinstance(v, int):
                        throughput_bytes = v
                        break
            break

    return Estimate(mean_seconds=float(mean_ns) / 1e9, throughput_bytes=throughput_bytes)


def _iter_bench_dirs(root: Path) -> Iterable[Path]:
    for p in root.rglob("estimates.json"):
        if p.parent.name != "new":
            continue
        yield p.parent.parent


def _bench_name(root: Path, bench_dir: Path) -> str:
    rel = bench_dir.relative_to(root)
    return "/".join(rel.parts)


def _iter_bench_dirs_cargo_criterion(root: Path) -> Iterable[Path]:
    # Layout produced by `cargo criterion`:
    #   target/criterion/data/<profile>/<bench>/.../benchmark.cbor
    data_dir = root / "data"
    if not data_dir.exists():
        return
    for p in data_dir.rglob("benchmark.cbor"):
        yield p.parent


def _bench_name_cargo_criterion(root: Path, bench_dir: Path) -> str:
    # Strip leading "data/<profile>/".
    rel = bench_dir.relative_to(root)
    parts = list(rel.parts)
    if len(parts) >= 3 and parts[0] == "data":
        parts = parts[2:]
    return "/".join(parts)


def _load_estimate_cargo_criterion(bench_dir: Path) -> Optional[Estimate]:
    benchmark_path = bench_dir / "benchmark.cbor"
    if not benchmark_path.exists():
        return None

    bench_meta = _read_cbor(benchmark_path)
    if not isinstance(bench_meta, dict):
        return None

    latest_record = bench_meta.get("latest_record")
    if not isinstance(latest_record, str):
        return None

    measurement_path = bench_dir / latest_record
    if not measurement_path.exists():
        return None

    measurement = _read_cbor(measurement_path)
    if not isinstance(measurement, dict):
        return None

    mean_ns = _get_nested(measurement, ["estimates", "mean", "point_estimate"])
    if mean_ns is None:
        mean_ns = _get_nested(measurement, ["estimates", "median", "point_estimate"])
    if mean_ns is None:
        return None

    throughput_bytes: Optional[int] = None
    thr = measurement.get("throughput")
    if isinstance(thr, dict):
        v = thr.get("Bytes")
        if isinstance(v, int):
            throughput_bytes = v

    return Estimate(mean_seconds=float(mean_ns) / 1e9, throughput_bytes=throughput_bytes)


def _key_for_pair(name: str) -> Optional[Tuple[str, str]]:
    # returns (impl, key-with-placeholder)
    parts = name.split("/")
    if "mcapable_bufwriter" in parts:
        impl = "mcapable_bufwriter"
    elif "mcap_crate_bufwriter" in parts:
        impl = "mcap_crate_bufwriter"
    elif "mcapable_slice" in parts:
        impl = "mcapable_slice"
    elif "mcapable" in parts:
        impl = "mcapable"
    elif "mcap_crate" in parts:
        impl = "mcap_crate"
    else:
        return None
    key = "/".join(
        "{impl}"
        if p
        in (
            "mcapable",
            "mcapable_slice",
            "mcapable_bufwriter",
            "mcap_crate",
            "mcap_crate_bufwriter",
        )
        else p
        for p in parts
    )
    return impl, key


def _colorize(s: str, color: str, enabled: bool) -> str:
    if not enabled:
        return s
    return f"{color}{s}{ANSI_RESET}"


def _format_cell(est: Estimate) -> str:
    t = _human_time(est.mean_seconds)
    if est.throughput_bytes is None:
        return t
    bps = est.throughput_bytes / est.mean_seconds if est.mean_seconds > 0 else 0.0
    return f"{t} ({_human_bytes_per_sec(bps)})"


def _render_table(rows: List[Tuple[str, str, str, str, str, str, str, str, str]]) -> str:
    headers = (
        "benchmark",
        "mcapable",
        "mcapable_slice",
        "mcap_crate",
        "diff",
        "diff_slice",
        "mcapable_bufwriter",
        "mcap_crate_bufwriter",
        "diff_bufwriter",
    )
    widths = [len(h) for h in headers]
    for r in rows:
        for i, c in enumerate(r):
            widths[i] = max(widths[i], len(c))

    def fmt_row(cols: Tuple[str, str, str, str, str, str, str, str, str]) -> str:
        return "  ".join(c.ljust(widths[i]) for i, c in enumerate(cols))

    out = [fmt_row(headers), fmt_row(tuple("-" * w for w in widths))]
    out.extend(fmt_row(r) for r in rows)
    return "\n".join(out)


def _diff_class(diff_str: str) -> str:
    if not diff_str or diff_str == "-":
        return ""
    s = diff_str.strip()
    try:
        # Strip ANSI if present.
        for c in (ANSI_GREEN, ANSI_RED, ANSI_YELLOW, ANSI_DIM, ANSI_RESET):
            s = s.replace(c, "")
        pct = float(s.replace("%", ""))
    except Exception:
        return ""
    if pct < -1.0:
        return "better"
    if pct > 1.0:
        return "worse"
    return "neutral"


def _render_html(
    rows: List[Tuple[str, str, str, str, str, str, str, str, str]],
    *,
    title: str,
) -> str:
    headers = (
        "test",
        "size",
        "chunking",
        "compression",
        "benchmark",
        "mcapable",
        "mcapable_slice",
        "mcap_crate",
        "diff",
        "diff_slice",
        "mcapable_bufwriter",
        "mcap_crate_bufwriter",
        "diff_bufwriter",
    )

    def parse_benchmark(b: str) -> Tuple[str, str, str, str]:
        # Expected shape: "<test>/<case>" where case often looks like:
        #  - "small_chunked-zstd"
        #  - "medium_unchunked-none"
        parts = b.split("/")
        test = "/".join(parts[:-1]) if len(parts) > 1 else b
        case = parts[-1] if len(parts) > 1 else ""

        size = "unknown"
        chunking = "unknown"
        compression = "unknown"

        if case:
            if "/" in case:
                # Older style: "small/chunked-zstd"
                cparts = case.split("/")
                if len(cparts) >= 2:
                    size = cparts[0]
                    rest = cparts[1]
                else:
                    rest = case
            elif "_" in case:
                # Newer style: "small_chunked-zstd"
                size, rest = case.split("_", 1)
            else:
                rest = case

            if rest.startswith("chunked-"):
                chunking = "chunked"
                compression = rest.removeprefix("chunked-")
            elif rest.startswith("unchunked-"):
                chunking = "unchunked"
                compression = rest.removeprefix("unchunked-")

        return test, size, chunking, compression

    def td(val: str, cls: str = "") -> str:
        v = html.escape(val)
        c = f' class="{cls}"' if cls else ""
        return f"<td{c}>{v}</td>"

    def th(val: str) -> str:
        return f"<th>{html.escape(val)}</th>"

    body_rows: List[str] = []
    for r in rows:
        test, size, chunking, compression = parse_benchmark(r[0])
        diff_cls = _diff_class(r[4])
        diff_slice_cls = _diff_class(r[5])
        diff_buf_cls = _diff_class(r[8])
        body_rows.append(
            "<tr>"
            + td(test, "dim")
            + td(size, "dim")
            + td(chunking, "dim")
            + td(compression, "dim")
            + td(r[0], "bench")
            + td(r[1], "num")
            + td(r[2], "num")
            + td(r[3], "num")
            + td(r[4], f"num diff {diff_cls}".strip())
            + td(r[5], f"num diff {diff_slice_cls}".strip())
            + td(r[6], "num")
            + td(r[7], "num")
            + td(r[8], f"num diff {diff_buf_cls}".strip())
            + "</tr>"
        )

    return f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{html.escape(title)}</title>
  <style>
    :root {{
      --bg: #0b0d10;
      --fg: #e6e6e6;
      --muted: #9aa4b2;
      --border: #262b33;
      --better: #12361a;
      --worse: #3a1515;
      --neutral: #2b2a18;
    }}
    body {{
      margin: 0;
      background: var(--bg);
      color: var(--fg);
      font: 14px/1.4 ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial;
    }}
    header {{
      padding: 14px 18px;
      border-bottom: 1px solid var(--border);
    }}
    header h1 {{
      margin: 0;
      font-size: 16px;
      font-weight: 600;
    }}
    header .meta {{
      margin-top: 4px;
      color: var(--muted);
      font-size: 12px;
    }}
    main {{ padding: 14px 18px; }}
    table {{
      width: 100%;
      border-collapse: collapse;
      table-layout: auto;
    }}
    th, td {{
      border: 1px solid var(--border);
      padding: 6px 8px;
      vertical-align: top;
    }}
    th {{
      position: sticky;
      top: 0;
      background: #0f1318;
      text-align: left;
      font-weight: 600;
      cursor: pointer;
      user-select: none;
    }}
    td.num {{ white-space: nowrap; font-variant-numeric: tabular-nums; }}
    td.bench {{ font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace; }}
    td.dim {{ color: var(--muted); white-space: nowrap; }}
    td.diff.better {{ background: var(--better); }}
    td.diff.worse {{ background: var(--worse); }}
    td.diff.neutral {{ background: var(--neutral); }}
  </style>
</head>
<body>
  <header>
    <h1>{html.escape(title)}</h1>
    <div class="meta">Columns: {html.escape(", ".join(headers))}</div>
    <div class="meta" style="margin-top:10px; display:flex; flex-wrap:wrap; gap:10px; align-items:center;">
      <label style="display:flex; gap:6px; align-items:center;">
        <span style="color:var(--muted); font-size:12px;">Search</span>
        <input id="filter-search" type="text" placeholder="type to filter…" style="padding:6px 8px; background:#0f1318; color:var(--fg); border:1px solid var(--border); border-radius:6px; min-width:260px;" />
      </label>
      <label style="display:flex; gap:6px; align-items:center;">
        <input id="filter-worse-only" type="checkbox" />
        <span style="color:var(--muted); font-size:12px;">Only regressions</span>
      </label>
      <label style="display:flex; gap:6px; align-items:center;">
        <span style="color:var(--muted); font-size:12px;">Min |diff|%</span>
        <input id="filter-threshold" type="number" min="0" step="0.1" value="0" style="width:90px; padding:6px 8px; background:#0f1318; color:var(--fg); border:1px solid var(--border); border-radius:6px;" />
      </label>
      <button id="filter-reset" style="padding:6px 10px; background:#0f1318; color:var(--fg); border:1px solid var(--border); border-radius:6px; cursor:pointer;">Reset</button>
      <span id="filter-count" style="color:var(--muted); font-size:12px;"></span>
    </div>
  </header>
  <main>
    <table id="bench-table">
      <thead><tr>{''.join(th(h) for h in headers)}</tr></thead>
      <tbody>
        {''.join(body_rows)}
      </tbody>
    </table>
  </main>
  <script>
    const table = document.getElementById("bench-table");
    const thead = table.querySelector("thead");
    const tbody = table.querySelector("tbody");
    const headers = Array.from(thead.querySelectorAll("th"));
    const filterSearch = document.getElementById("filter-search");
    const filterWorseOnly = document.getElementById("filter-worse-only");
    const filterThreshold = document.getElementById("filter-threshold");
    const filterReset = document.getElementById("filter-reset");
    const filterCount = document.getElementById("filter-count");

    const LS_KEY = "bench_summary_state_v2";

    function parsePercent(text) {{
      const t = (text || "").replace(/\\s+/g, "").replace("%", "");
      const n = Number.parseFloat(t);
      return Number.isFinite(n) ? n : Number.POSITIVE_INFINITY;
    }}

    function parseDurationSeconds(text) {{
      const m = (text || "").trim().match(/^([0-9]*\\.?[0-9]+)\\s*(ns|µs|us|ms|s)\\b/);
      if (!m) return Number.POSITIVE_INFINITY;
      const v = Number.parseFloat(m[1]);
      const u = m[2];
      if (!Number.isFinite(v)) return Number.POSITIVE_INFINITY;
      if (u === "ns") return v * 1e-9;
      if (u === "µs" || u === "us") return v * 1e-6;
      if (u === "ms") return v * 1e-3;
      return v;
    }}

    function inferType(colName) {{
      if (colName.startsWith("diff")) return "percent";
      if (colName === "mcapable" || colName === "mcapable_slice" || colName === "mcap_crate" ||
          colName === "mcapable_bufwriter" || colName === "mcap_crate_bufwriter") return "duration";
      return "text";
    }}

    function getCellValue(row, idx, type) {{
      const cell = row.children[idx];
      const text = cell ? cell.textContent : "";
      if (type === "percent") return parsePercent(text);
      if (type === "duration") return parseDurationSeconds(text);
      return (text || "").toLowerCase();
    }}

    function loadState() {{
      try {{
        const raw = localStorage.getItem(LS_KEY);
        if (!raw) return null;
        return JSON.parse(raw);
      }} catch (_) {{
        return null;
      }}
    }}

    function saveState(state) {{
      try {{
        localStorage.setItem(LS_KEY, JSON.stringify(state));
      }} catch (_) {{}}
    }}

    // sortKeys: (idx, asc) pairs in priority order.
    let sortKeys = [{{ idx: 0, asc: true }}];

    function sortRows() {{
      const rows = Array.from(tbody.querySelectorAll("tr"));

      rows.sort((a, b) => {{
        for (const key of sortKeys) {{
          const name = headers[key.idx].textContent.trim().replace(/\\s+[▲▼]$/, "");
          const type = inferType(name);
          const va = getCellValue(a, key.idx, type);
          const vb = getCellValue(b, key.idx, type);
          if (va < vb) return key.asc ? -1 : 1;
          if (va > vb) return key.asc ? 1 : -1;
        }}
        // stable tie-breaker: benchmark column
        const ba = getCellValue(a, 4, "text");
        const bb = getCellValue(b, 4, "text");
        if (ba < bb) return -1;
        if (ba > bb) return 1;
        return 0;
      }});

      for (const r of rows) tbody.appendChild(r);
      for (const [i, h] of headers.entries()) {{
        const base = h.textContent.replace(/\\s+[▲▼]$/, "");
        const key = sortKeys.find(k => k.idx === i);
        if (key) h.textContent = base + (key.asc ? " ▲" : " ▼");
        else h.textContent = base;
      }}
    }}

    function setSort(idx, multi) {{
      if (!multi) {{
        const existing = sortKeys.find(k => k.idx === idx);
        const asc = existing ? !existing.asc : true;
        sortKeys = [{{ idx, asc }}];
      }} else {{
        const existing = sortKeys.find(k => k.idx === idx);
        if (existing) {{
          existing.asc = !existing.asc;
        }} else {{
          sortKeys.push({{ idx, asc: true }});
        }}
      }}
      sortRows();
      persistUiState();
    }}

    function rowDiffPercents(row) {{
      // diff columns are 8,9,12 in the current table layout
      const idxs = [8, 9, 12];
      const out = [];
      for (const i of idxs) {{
        const v = parsePercent(row.children[i]?.textContent || "");
        if (Number.isFinite(v) && v !== Number.POSITIVE_INFINITY) out.push(v);
      }}
      return out;
    }}

    function applyFilters() {{
      const q = (filterSearch.value || "").trim().toLowerCase();
      const worseOnly = !!filterWorseOnly.checked;
      const threshold = Number.parseFloat(filterThreshold.value || "0") || 0;

      let shown = 0;
      const rows = Array.from(tbody.querySelectorAll("tr"));
      for (const row of rows) {{
        const text = (row.textContent || "").toLowerCase();
        if (q && !text.includes(q)) {{
          row.style.display = "none";
          continue;
        }}

        const diffs = rowDiffPercents(row);
        const hasWorse = diffs.some(d => d > 0);
        const meetsThreshold = diffs.some(d => Math.abs(d) >= threshold);

        if (worseOnly && !hasWorse) {{
          row.style.display = "none";
          continue;
        }}
        if (threshold > 0 && !meetsThreshold) {{
          row.style.display = "none";
          continue;
        }}

        row.style.display = "";
        shown += 1;
      }}

      if (filterCount) {{
        filterCount.textContent = `${{shown}} / ${{rows.length}} rows`;
      }}
    }}

    function persistUiState() {{
      saveState({{
        search: filterSearch.value || "",
        worseOnly: !!filterWorseOnly.checked,
        threshold: filterThreshold.value || "0",
        sortKeys,
      }});
    }}

    function restoreUiState() {{
      const s = loadState();
      if (!s) return;
      if (typeof s.search === "string") filterSearch.value = s.search;
      if (typeof s.worseOnly === "boolean") filterWorseOnly.checked = s.worseOnly;
      if (typeof s.threshold === "string" || typeof s.threshold === "number") filterThreshold.value = String(s.threshold);
      if (Array.isArray(s.sortKeys) && s.sortKeys.length > 0) {{
        sortKeys = s.sortKeys.filter(k => Number.isFinite(k.idx) && typeof k.asc === "boolean");
        if (sortKeys.length === 0) sortKeys = [{{ idx: 0, asc: true }}];
      }}
    }}

    headers.forEach((h, idx) => {{
      h.addEventListener("click", (ev) => setSort(idx, ev.shiftKey));
    }});

    filterSearch.addEventListener("input", () => {{ applyFilters(); persistUiState(); }});
    filterWorseOnly.addEventListener("change", () => {{ applyFilters(); persistUiState(); }});
    filterThreshold.addEventListener("input", () => {{ applyFilters(); persistUiState(); }});
    filterReset.addEventListener("click", () => {{
      filterSearch.value = "";
      filterWorseOnly.checked = false;
      filterThreshold.value = "0";
      sortKeys = [{{ idx: 0, asc: true }}];
      sortRows();
      applyFilters();
      persistUiState();
    }});

    restoreUiState();
    sortRows();
    applyFilters();
  </script>
</body>
</html>
"""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path, default=Path("target/criterion"))
    ap.add_argument(
        "--filter",
        default="",
        help="Only include benchmarks whose name contains this substring.",
    )
    ap.add_argument("--no-color", action="store_true")
    ap.add_argument(
        "--format",
        choices=["text", "html"],
        default="text",
        help="Output format: text (terminal) or html (standalone page).",
    )
    ap.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Write output to this file instead of stdout.",
    )
    ap.add_argument(
        "--sort",
        choices=["name", "diff"],
        default="diff",
        help="Sort output by benchmark name or by time diff.",
    )
    args = ap.parse_args()

    root = args.root
    if not root.exists():
        print(f"error: root not found: {root}", file=sys.stderr)
        return 2

    color = (not args.no_color) and _is_tty() and args.format == "text"

    by_key: Dict[str, Dict[str, Estimate]] = {}

    # Prefer `cargo criterion`'s CBOR format if present; fall back to Criterion's
    # JSON output format (estimates.json) used by `cargo bench`.
    if (root / "data").exists():
        candidates = (
            (_bench_name_cargo_criterion(root, d), _load_estimate_cargo_criterion(d))
            for d in _iter_bench_dirs_cargo_criterion(root)
        )
    else:
        candidates = ((_bench_name(root, d), _load_estimate(d)) for d in _iter_bench_dirs(root))

    for name, est in candidates:
        if est is None:
            continue
        if args.filter:
            f = args.filter
            if f not in name and f.replace("/", "_") not in name:
                continue

        pair = _key_for_pair(name)
        if pair is None:
            continue
        impl, key = pair
        by_key.setdefault(key, {})[impl] = est

    rows: List[Tuple[str, str, str, str, str, str, str, str, str]] = []
    sortable: List[Tuple[float, Tuple[str, str, str, str, str, str, str, str, str]]] = []

    def fmt_diff(diff_pct: float) -> str:
        s = _pct(diff_pct)
        if diff_pct < -1.0:
            return _colorize(s, ANSI_GREEN, color)
        if diff_pct > 1.0:
            return _colorize(s, ANSI_RED, color)
        return _colorize(s, ANSI_YELLOW, color)

    for key, impls in by_key.items():
        c = impls.get("mcap_crate")
        cb = impls.get("mcap_crate_bufwriter")
        m = impls.get("mcapable")
        ms = impls.get("mcapable_slice")
        mb = impls.get("mcapable_bufwriter")

        if c is None and cb is None:
            continue
        if m is None and ms is None and mb is None:
            continue

        parts = [p for p in key.split("/") if p != "{impl}"]
        bench_name = "/".join(parts)

        if c is not None and c.mean_seconds > 0 and m is not None:
            diff_pct = (m.mean_seconds - c.mean_seconds) / c.mean_seconds * 100.0
            diff_str = fmt_diff(diff_pct)
        else:
            diff_pct = 0.0
            diff_str = "-"

        if c is not None and c.mean_seconds > 0 and ms is not None:
            diff_slice_pct = (ms.mean_seconds - c.mean_seconds) / c.mean_seconds * 100.0
            diff_slice_str = fmt_diff(diff_slice_pct)
        else:
            diff_slice_pct = diff_pct
            diff_slice_str = "-"

        if cb is not None and cb.mean_seconds > 0 and mb is not None:
            diff_buf_pct = (mb.mean_seconds - cb.mean_seconds) / cb.mean_seconds * 100.0
            diff_buf_str = fmt_diff(diff_buf_pct)
        else:
            diff_buf_pct = diff_pct
            diff_buf_str = "-"

        row = (
            bench_name,
            _format_cell(m) if m is not None else "-",
            _format_cell(ms) if ms is not None else "-",
            _format_cell(c) if c is not None else "-",
            diff_str,
            diff_slice_str,
            _format_cell(mb) if mb is not None else "-",
            _format_cell(cb) if cb is not None else "-",
            diff_buf_str,
        )

        if m is not None and c is not None:
            sort_key = diff_pct
        elif mb is not None and cb is not None:
            sort_key = diff_buf_pct
        else:
            sort_key = diff_slice_pct
        sortable.append((sort_key, row))

    if args.sort == "name":
        rows = [r for _, r in sorted(sortable, key=lambda x: x[1][0])]
    else:
        rows = [r for _, r in sorted(sortable, key=lambda x: x[0])]

    if not rows:
        msg = f"no matching paired benchmarks found under {root}"
        if args.format == "html":
            out = _render_html([], title=msg)
        else:
            out = f"{ANSI_DIM if color else ''}{msg}{ANSI_RESET if color else ''}"
        if args.output is not None:
            args.output.write_text(out, encoding="utf-8")
        else:
            print(out)
        return 0

    if args.format == "html":
        out = _render_html(rows, title=f"bench_summary ({args.filter or 'all'})")
    else:
        out = _render_table(rows)

    if args.output is not None:
        args.output.write_text(out, encoding="utf-8")
    else:
        print(out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
