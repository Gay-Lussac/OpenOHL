# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Validate every wire library (*.ohlw, the older *.olsw, and the
template) against the JSON Schema, so the schema documented for tool
developers matches real files.

Usage: python3 check_wire_schema.py   (needs Python 3.11+ and `jsonschema`)
"""

import json
import pathlib
import sys
import tomllib

import jsonschema

root = pathlib.Path(__file__).resolve().parents[2]
schema = json.loads((root / "docs/schema/ohl-wire-1.schema.json").read_text())
validator = jsonschema.Draft202012Validator(schema)
files = sorted(root.glob("**/*.ohlw")) + sorted(root.glob("**/*.olsw"))
files += [root / "libraries/cables/TEMPLATE.ohlw.example"]
files = [f for f in files if "target" not in f.parts]
failures = 0
for f in files:
    data = tomllib.loads(f.read_text())
    errors = list(validator.iter_errors(data))
    for e in errors:
        print(f"{f.relative_to(root)}: {'/'.join(map(str, e.absolute_path))}: {e.message}")
    failures += len(errors)
    print(f"{'ok  ' if not errors else 'FAIL'} {f.relative_to(root)}")
sys.exit(1 if failures else 0)
