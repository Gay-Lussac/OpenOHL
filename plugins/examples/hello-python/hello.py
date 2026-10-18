# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Example OpenOHL Python-tier plugin/script.

Run with the `ohl` Python package installed (see crates/ohl-py):

    python plugins/examples/hello-python/hello.py
"""

import ohl

PLUGIN = {
    "name": "hello-python",
    "version": "0.1.0",
    "description": "Example Python plugin: prints a span table in US units",
}


def span_table(spans_m, load_n_per_m):
    """Total distributed load per span, converted to US customary units."""
    rows = []
    for span in spans_m:
        length_ft = ohl.convert("length", f"{span} m", "ft")
        total_lbf = ohl.convert("force", f"{span * load_n_per_m} N", "lbf")
        rows.append((span, length_ft, total_lbf))
    return rows


if __name__ == "__main__":
    print(f"OpenOHL {ohl.__version__} — {PLUGIN['description']}")
    for span, feet, lbf in span_table([300, 350, 410], 16.0):
        print(f"{span:6.0f} m = {feet:8.1f} ft   load {lbf:8.0f} lbf")
