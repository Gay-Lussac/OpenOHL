# Design-code plugins

One WebAssembly plugin per design code (`code-plugin` world, extension
point `design-code`). Each reads **your parameter file** (TOML) and
generates weather cases (`.ohlwc`) and load cases (`.ohllc`) with notes
(and may give presets of the clearance windows; these plugins give none).
They are built for the `code-rules-plugin` world: their optional
`structure-wind` (wind factors for structures: drag per shape, a lattice
face's drag and shielding by solidity) is a stub returning **none**
(`no_structure_wind!()` in `common/`), so OpenOHL keeps its generic drag
factor and no shielding and says so; their optional `rule-presets`
(presets of any clearance rule set) is a stub returning none too
(`no_rule_presets!()`).

| Plugin | Code | Units of the parameter file |
| --- | --- | --- |
| `en50341` | EN 50341-1 general approach + national annex (NNA) overlay | SI |
| `iec60826` | IEC 60826 reliability-based loading | SI |
| `nesc` | NESC (ANSI C2) Rule 250 | US customary |
| `csa-c22-3` | CSA C22.3 No. 1 loading zones and construction grades | SI |
| `asce-mop74` | ASCE MOP 74 structural loading | SI |
| `go95` | CPUC General Order 95, Rules 43 and 44 | US customary |

**OpenOHL ships no code tables.** The plugins implement each code's
method; every wind speed, ice thickness, temperature, partial factor and
combination factor comes from the parameter file. The
`example-parameters.toml` of each plugin (also its template) holds
**invented placeholder values** — not from any standard. Replace each one
from the standard and national annex you design to.

A plugin for the Swiss LeV / OLEI (SR 734.31) exists separately; it is
not part of the public code.

`common/` is shared by the plugins (parameter files, writing the files,
clearance presets).

Build one (or all) for WebAssembly:

```bash
rustup target add wasm32-wasip2
cd plugins/codes/en50341 && cargo build --release --target wasm32-wasip2
```

Use it:

```bash
ohl code list
ohl code template --plugin plugins/codes/en50341 --out my-line.toml
ohl code generate --plugin plugins/codes/en50341 --params my-line.toml \
    --weather my-line.ohlwc --load-cases my-line.ohllc
```

or in the desktop application: Criteria › Design code… (the plugin
installed or its folder added in Plugins › Plugin manager). See
`docs/user-guide/design-codes.md`.
