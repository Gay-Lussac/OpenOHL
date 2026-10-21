# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Compare two span tables written by `ohl line ... --spans-csv`.

Typical use: the same line and weather case solved by two builds (before
and after a change), or OpenOHL against another program's results put in
the same columns. Rows are matched by (section, span); every column both
files have is compared.

    python3 -I tools/compare_spans.py BEFORE.csv AFTER.csv [--top N] [--csv OUT.csv]
    python3 -I tools/compare_spans.py BEFORE_DIR AFTER_DIR [--top N]

With two directories, every CSV file name present in both is compared
(one weather case per file), and a summary per file is printed.

Printed per file: the number of spans, and for each column the largest
change (absolute, and relative for tensions), the median absolute change,
and the span where the largest change is. `--top N` lists the N spans with
the largest sag change. `--csv` writes every span's changes (one file
only). Standard library only; reads only the files it is given.
"""

import csv
import os
import statistics
import sys

KEY = ("section", "span")


def read(path):
    with open(path, newline="") as f:
        rows = list(csv.DictReader(f))
    out = {}
    for r in rows:
        out[(r["section"], r["span"])] = r
    return out, (list(rows[0].keys()) if rows else [])


def number(text):
    try:
        return float(text)
    except (TypeError, ValueError):
        return None


def compare(a_path, b_path, top=0, out_csv=None):
    a, cols_a = read(a_path)
    b, cols_b = read(b_path)
    cols = [c for c in cols_a if c in cols_b and c not in KEY]
    keys = [k for k in a if k in b]
    only = len(a) + len(b) - 2 * len(keys)
    print(f"{os.path.basename(a_path)} -> {os.path.basename(b_path)}: {len(keys)} spans"
          + (f" ({only} in one file only)" if only else ""))
    rows = []
    for k in keys:
        row = {"section": k[0], "span": k[1]}
        for c in cols:
            x, y = number(a[k][c]), number(b[k][c])
            if x is None or y is None:
                continue
            row[c] = y - x
            row[c + "_before"] = x
        rows.append(row)
    for c in cols:
        d = [(abs(r[c]), r) for r in rows if c in r]
        if not d:
            continue
        worst, r = max(d, key=lambda t: t[0])
        med = statistics.median(x for x, _ in d)
        rel = ""
        if c.endswith("_n") and r[c + "_before"]:
            rel = f" ({r[c] / r[c + '_before'] * 100:+.4f} %)"
        print(f"  {c:<20} largest {r[c]:+.4f}{rel} at {r['section']} span {r['span']}, median |change| {med:.4f}")
    if top:
        ranked = sorted((r for r in rows if "sag_m" in r), key=lambda r: -abs(r["sag_m"]))[:top]
        for r in ranked:
            h = r.get("horizontal_tension_n")
            print(f"    {r['section']} span {r['span']}: sag {r['sag_m'] * 1000:+.1f} mm"
                  + (f", H {h:+.2f} N" if h is not None else ""))
    if out_csv:
        fields = ["section", "span"] + [c for c in cols if any(c in r for r in rows)]
        with open(out_csv, "w", newline="") as f:
            w = csv.DictWriter(f, fieldnames=fields, extrasaction="ignore")
            w.writeheader()
            w.writerows(rows)
    return rows


def main(argv):
    args = [x for x in argv if not x.startswith("--")]
    top = int(argv[argv.index("--top") + 1]) if "--top" in argv else 0
    out_csv = argv[argv.index("--csv") + 1] if "--csv" in argv else None
    if out_csv in args:
        args.remove(out_csv)
    if "--top" in argv:
        args.remove(argv[argv.index("--top") + 1])
    if len(args) != 2:
        print(__doc__)
        return 2
    a, b = args
    if os.path.isdir(a) and os.path.isdir(b):
        names = sorted(n for n in os.listdir(a) if n.endswith(".csv") and os.path.isfile(os.path.join(b, n)))
        for n in names:
            compare(os.path.join(a, n), os.path.join(b, n), top)
    else:
        compare(a, b, top, out_csv)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
