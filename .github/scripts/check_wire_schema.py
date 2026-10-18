# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
