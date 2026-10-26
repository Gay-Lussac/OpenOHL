# Design codes

A **design-code plugin** turns your **parameter file** into weather cases
([`.ohlwc`](weather.md)) and load cases ([`.ohllc`](loads.md)) for a line:
the cases the code asks for, the wind pressures and ice loads computed
the way the code computes them, the partial and combination factors put
on the vertical, transverse and longitudinal loads, and the strength
factor. The plugin also returns **notes**: what it computed, assumed or
left out, and may give **clearance presets** for the Vertical and Radial
clearances windows ([below](#clearance-presets)) and **rule presets** for
any clearance rule set ([below](#rule-presets)).

**OpenOHL ships no code tables.** Standards are copyrighted, and their
values change with editions and national annexes. Each plugin implements
a code's *method*; every number — basic wind speeds, ice thicknesses,
temperatures, γ and ψ factors, safety factors, district or zone values —
is read from your parameter file, which you fill in from the standard you
hold. Each plugin's `example-parameters.toml` (also the template it
gives) uses **invented placeholder values**, clearly marked: replace them
all before using the cases.

## The plugins

They live in `plugins/codes/`, one per code, each with a README listing
its parameter file keys:

| Plugin | Code | Parameter units |
| --- | --- | --- |
| `en50341` | EN 50341-1 general approach, with a national annex (NNA) overlay | SI |
| `iec60826` | IEC 60826 (reliability-based) | SI |
| `nesc` | NESC (ANSI C2) Rule 250 | US customary |
| `csa-c22-3` | CSA C22.3 No. 1 | SI |
| `asce-mop74` | ASCE Manual of Practice 74 | SI |
| `go95` | CPUC General Order 95 (Rules 43, 44) | US customary |

Build them once (they are WebAssembly components):

```bash
rustup target add wasm32-wasip2
for d in plugins/codes/*/; do [ -f "$d/ohl-plugin.toml" ] && (cd "$d" && cargo build --release --target wasm32-wasip2); done
```

### EN 50341 and national annexes

The EN 50341 parameter file has the general values (wind, ice,
combined wind and ice, temperatures, partial factors) and an `[nna]`
section: any table of the general part given again under `nna`
(e.g. `[nna.wind]`) replaces the general values key by key, and
`[[nna.extra_weather]]` / `[[nna.extra_load_case]]` add the national
cases. The notes list the keys the annex replaced.

The wind pressure on conductors is
\\( q = c_e(h)\cdot\tfrac{1}{2}\rho\\,(f\\,v_b)^2\cdot G_c\cdot C_c \\),
with \\( f \\) = 1 for the extreme wind and the file's fraction for the
other cases. Combined wind and ice is generated twice: ice dominant
(\\( \gamma_I \\) on the ice, \\( \gamma_W\psi_W \\) on the wind) and wind
dominant (\\( \gamma_I\psi_I \\), \\( \gamma_W \\)). The strength factor
is \\( 1/\gamma_M \\).

A plugin for the Swiss Leitungsverordnung (LeV / OLEI, SR 734.31) exists
separately; it is not part of the public code.

## Clearance presets

A design-code plugin may also return **clearance presets**: for the
**Vertical clearances** or the **Radial clearances** window, a rule file
(`.ohlr`: the rules — LiDAR classes, voltages, distances, the vertical
zone's half width per rule —, the wind steps and sides (and the swing arc, `[wind] arc = true`), the circuits,
the calculation method and the distance beyond which LiDAR points are
ignored) and the weather cases its rules name (`.ohlwc`). OpenOHL checks
them like any such file (each rule's weather cases among the preset's),
and writes each as `<plugin>-<name>.ohlr` among the project's rule files
and `<plugin>-<name>.ohlwc` among its weather case files (next to a line
file; `ohl code generate`: next to the weather case file). In a window,
**Load preset…** takes one in a click: its rules replace the window's,
its weather cases are matched to the project's ([A preset's weather
cases](#a-presets-weather-cases)) and those with no equal created; you can change
everything afterwards (see [Vertical and radial
clearances](desktop-app.md#vertical-and-radial-clearances)). The plugins
in `plugins/codes` give none.

## Rule presets

A design-code plugin may also give **rule presets** (its optional
`rule-presets`, from the same parameter file — or none, when the plugin
takes its values from its own data): presets of **any clearance rule
set** — the ground, objects (LiDAR classes), crossings with other lines,
the line's own wires — each a rule file (`.ohlr`) with a description (what
it holds, the articles of the code it follows) and, when its rules name
weather cases, a weather case file. The distances use the forms of the
rule files: one distance, a table by voltage, or a base plus an amount
per kV ([Clearances by voltage](terrain.md#clearances-by-voltage)), so a
preset covers every voltage. OpenOHL checks each preset like any rule file
(at least one rule; each weather case a rule names among the preset's;
names unique).

In the desktop application the Clearances tab's **Edit rules › Load
preset…** lists the rule presets of the design-code plugins turned on,
read with the parameter file chosen in Line › Criteria › Design code (none
chosen: an empty one), shows one with its distances at the project's
voltages, and **merges** it into the rule file chosen (a rule of the same
name replaced, the others added) or **replaces** its rules — one command,
undone by Undo ([Clearances](desktop-app.md#clearances)). From the
command line:

```bash
ohl code rule-presets --plugin DIR [--params my-line.toml] [--out rules/]
```

lists each preset with its rules and their distances, and with `--out`
writes them as `<plugin>-rules-<name>.ohlr` (and `.ohlwc` for their
weather cases); `ohl code generate` writes them next to the weather case
file too. The plugins in `plugins/codes` give none (a stub). These
commands only write files: none puts a preset's cases into a project.

### Presets per check

A preset brings **only what the check it is applied in needs**. The
vegetation panel lists the presets with vegetation settings, the
Vertical clearances window those with vertical rules, the Radial window
those with radial rules, the rule editor every preset. Applied in the
Vertical window, a preset gives its vertical rules and the weather cases
those rules name; in the Radial window its radial rules and theirs; in
the vegetation panel its vegetation settings, its wind steps and its own
weather cases — never a case only another check uses. So one preset may
hold the rules of many checks (dozens of rules, each with its own cases)
and the vegetation settings together. The previews group the rules by
what they are to (the ground, LiDAR classes, the line's own wires, other
lines) and give per rule the weather cases it uses. A preset's rule file
may also say which checks it is for (`checks = ["vertical", "radial",
"vegetation", "tension", "strings", "internal"]`; `tension`: its `[[tension_rule]]` tables for
the [wire tension check](desktop-app.md#wire-tensions), 2026-10-22;
`strings`: its `[[string_rule]]` tables for the [string
check](desktop-app.md#string-checks), 2026-10-24; `internal`: its
`[[internal_rule]]` tables for the [internal
clearances](terrain.md#internal-clearance-rules-internal_rule), 2026-10-25) and the vegetation settings which cases they run in
(`cases` under `[vegetation]`); see
[Plugins](../development/plugins.md#presets-per-check).

### A preset's weather cases

When a preset is applied in the desktop application — the rule editor's
Load preset…, the Vertical and Radial clearance windows (Load preset…
and Plugin preset), the vegetation panel — its weather cases are
matched to the project's **by weather first, then by name** (the owner's
rule of 2026-10-21):

- A preset case with **the same weather** as one of your cases — whatever
  the names — **uses your case**: the preset's rules are rewritten to
  name it (a rule naming "Utility 80 °C" names your "80 °C"), and it is
  ticked for the run. Nothing is created. When several of your cases are
  the same, the **first in your list** is used, and the preview says so.
- Only a case none of yours equals is **created**. If you already have a
  case of its name with other values, yours is kept and the preset's is
  created as **"name (preset)"**, its rules naming it so.

The previews list each preset case as *uses your case X* or *creates Y*;
the status after applying repeats it. The same weather means: the same
wire temperature; the same ice — none on both (a thickness or a load of
0), or both a radial thickness with the same thickness and density, or
both a load per metre with the same load and density (a density not
given is glaze ice, 913 kg/m³, as the loads take it); the same wind
pressure, and with wind the same side and air density factor; the same
ambient temperature. Numbers are compared within one part in a million.
The name, the description and the tags never matter. Ice given as a
thickness and ice given as a load are **never** the same, even when they
weigh the same on one wire: the weight of a thickness depends on the
wire's diameter. A factor not given (air density, ambient temperature)
equals only another not given.

## Wind factors for structures

A design-code plugin may also return the code's **wind factors for
structures** (its optional `structure-wind`, from the same parameter
file): the drag factor per profile shape, a lattice face's drag factor by
its solidity ratio, and the shielding factor of a face behind another by
the solidity ratio of the face in front (and the spacing ratio) — as a
cross-section library's `[wind]` table ([Section
library](section-library.md#wind-factors-for-structures-wind)). OpenOHL
checks it and writes it as `<plugin>-wind.ohlsec` into the **section
library** (the project line's, else the `Sections` folder next to the
line; `ohl code generate`: `--sections DIR`, default `Sections` beside the
weather case file); the finite-element check's member wind then uses it
([Wind on lattice structures](../theory/structure-wind.md)). OpenOHL finds
the faces, their solidity and which face shelters which member; the
numbers are the code's, from your parameter file. The plugins in
`plugins/codes` give **none** yet (their notes and the window say so):
the member wind keeps its one generic drag factor and no shielding.

## From the command line

```bash
ohl code list                               # plugins/codes, or --dir DIR
ohl code template --plugin plugins/codes/en50341 --out my-line.toml
# … fill in my-line.toml from the standard …
ohl code generate --plugin plugins/codes/en50341 --params my-line.toml \
    --weather my-line.ohlwc --load-cases my-line.ohllc
ohl line line.ohl.json --weather my-line.ohlwc --load-cases my-line.ohllc --loads-csv loads.csv
```

`generate` checks both files as OpenOHL reads them (and that each load
case names one of the weather cases) before writing them, writes the
clearance presets and the rule presets next to the weather case file and the wind factors for
structures, if the code gives them, into the cross-section library (`--sections
DIR`, default `Sections` beside the weather case file), and prints the
notes (and that the code gives no wind factors, when it gives none).

## In the desktop application

Install a code plugin or add its folder in File › Plugins › Plugin manager
(e.g. `plugins/codes`). Then **Line › Criteria › Design code…**:

1. choose the code;
2. **Save the plugin's template…** to start a parameter file, fill it in;
3. choose the **Parameter file…**;
4. **Generate**.

The two files (named as given, default the plugin's name) go into the
project's libraries — kept in the project file like any library file —
or next to a line file; a file of that name is replaced. With a weather
case file already in use, the code's cases are matched to yours by
weather first (2026-10-24, the owner's request: no case created twice):
one with the same weather as one of yours (temperature, ice, wind and
what else a calculation reads — not the name) **is yours**, and the
code's load cases name it; only the others are added, after your cases,
under their names (one taken by a case of other values: `"<name>
(preset)"`). The window says how many were already yours. Your own case
file is left as it is: the code's file holds your cases and the new
ones. The model then uses them: the weather cases are solved, and
Structure loads uses the load cases. The code's clearance presets go into the project's rule and
weather case files (for the clearance windows' Load preset…; its rule
presets are read when the Clearances tab's Load preset… opens), its wind
factors for structures into the project line's cross-section library. The
code's notes are shown in the window. In OHL-Structure, **Load cases ›
Load cases from a design code…** writes the wind factors into the section
library the structure reads when the files are written (the tick), and
says when the code gives none.

## What is not generated yet

Every plugin says in its notes what it leaves out. In this first version:
security and failure-containment loads (broken wire, unbalanced ice) —
the core solves a broken wire for any load case that names one (below),
the example plugins write none yet; construction and
maintenance loads; wind on structures by the code's own method (the
structure loads apply each weather case's wind pressure to the structure
types' height bands, see [Load cases](loads.md); the member wind of a
level 3 structure takes the code's drag and shielding factors only when
the plugin gives them, which these plugins do not yet); the NESC constant k (the notes give
its value to add by hand); per-wire ice loads that depend on the conductor
diameter (one reference diameter per parameter file).

**The structures' own wind.** A load case may give the structures a wind
pressure and factor of their own, apart from the wires'
(`[case.structure]`, [Load cases](loads.md#wind-on-the-structures-apart-from-the-wires-casestructure)).
A plugin supplies them in the load case file it generates — the
interface is unchanged (the `.ohllc` text carries them; the shared
`ohl-code-common` crate writes them with `Load::structure_wind` and
raises the file to version 2 only then). The example plugins give none
yet: their structures take the weather case's pressure and the
transverse factor.

**A broken wire** (roadmap O9; owner 2026-10-19: the core gets the
broken-wire case, the codes' security loads stay in their plugins). A
plugin writes a code's broken-wire cases as load cases with a
`[case.broken]` table in the `.ohllc` text it generates
([Load cases](loads.md#a-broken-wire-casebroken)) — the plugin
interface is unchanged:

```toml
format = "ohl-load-cases"
version = 3                # 3 only when a case has [case.broken]

[[case]]
name = "security: S3 every span"
description = "the code's clause, broken wire"
weather = "everyday"       # a weather case the plugin writes too
condition = "initial"
longitudinal = 1.0         # the code's impact factor on the unbalanced pull
tags = ["security"]

[case.broken]
section = "S3"             # a section of the line, by id
span = "every"             # or a span from 1
set = 1                    # optional check: the set of the wire
```

The line's sections and spans come from the line the cases are run on;
a plugin that does not know them writes `span = "every"` for the
sections it is told about (one case per section), or leaves the cases to
the user. The core solves the section with that wire gone, the strings
at its ends swung to the spans left, and gives the loads (with
`"every"`, the worst of each component and the span giving it). Not
modelled (in the [theory](../theory/wirefe.md#a-broken-wire-2026-10-25)):
the dynamic impact (the factor above), a single subconductor of a
bundle broken (the whole phase is).
