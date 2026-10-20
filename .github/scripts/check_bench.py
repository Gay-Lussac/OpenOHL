# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Fail if any benchmark got slower than a threshold versus the parent commit.

Both commits are benchmarked in the same CI job, on the same machine
(criterion baseline "parent" vs the current run), because absolute timings
from different runners are not comparable: shared runners have different
CPUs, and an unchanged f64 loop was measured 2.2x slower on another machine.

Usage: python3 check_bench.py <criterion_dir> <max_ratio>
Reads <criterion_dir>/**/change/estimates.json written by
`cargo bench -- --baseline-lenient parent`.
"""

import json
import os
import pathlib
import sys

root, threshold = pathlib.Path(sys.argv[1]), float(sys.argv[2])
rows, worst = [], 0.0
for path in sorted(root.glob("**/change/estimates.json")):
    name = path.parent.parent.relative_to(root).as_posix()
    ratio = 1.0 + json.loads(path.read_text())["mean"]["point_estimate"]
    worst = max(worst, ratio)
    flag = "❌" if ratio > threshold else "✅"
    rows.append(f"| {name} | {ratio:.3f}x | {flag} |")

if rows:
    report = ["## Benchmarks vs parent commit (same machine)", "",
              "| Benchmark | Time ratio | |", "|---|---|---|", *rows, "",
              f"Fails above {threshold}x."]
else:
    report = ["## Benchmarks", "", "No comparison available (no parent baseline)."]
text = "\n".join(report)
print(text)
summary = os.environ.get("GITHUB_STEP_SUMMARY")
if summary:
    with open(summary, "a") as f:
        f.write(text + "\n")
if worst > threshold:
    sys.exit(f"benchmark regression: {worst:.2f}x slower than the parent commit (limit {threshold}x)")
