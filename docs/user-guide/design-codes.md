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
its weather cases missing from the model are added to it; you can change
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
read with the parameter file chosen in Criteria › Design code (none
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
file too. The plugins in `plugins/codes` give none (a stub).

## Wind factors for structures

A design-code plugin may also return the code's **wind factors for
structures** (its optional `structure-wind`, from the same parameter
file): the drag factor per profile shape, a lattice face's drag factor by
its solidity ratio, and the shielding factor of a face behind another by
the solidity ratio of the face in front (and the spacing ratio) — as a
section library's `[wind]` table ([Section
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
structures, if the code gives them, into the section library (`--sections
DIR`, default `Sections` beside the weather case file), and prints the
notes (and that the code gives no wind factors, when it gives none).

## In the desktop application

Install a code plugin or add its folder in Plugins › Plugin manager
(e.g. `plugins/codes`). Then **Criteria › Design code…**:

1. choose the code;
2. **Save the plugin's template…** to start a parameter file, fill it in;
3. choose the **Parameter file…**;
4. **Generate**.

The two files (named as given, default the plugin's name) go into the
project's libraries — kept in the project file like any library file —
or next to a line file; a file of that name is replaced. The model then
uses them: the weather cases are solved, and Structure loads uses the
load cases. The code's clearance presets go into the project's rule and
weather case files (for the clearance windows' Load preset…; its rule
presets are read when the Clearances tab's Load preset… opens), its wind
factors for structures into the project line's section library. The
code's notes are shown in the window. In OHL-Structure, **Load cases ›
Load cases from a design code…** writes the wind factors into the section
library the structure reads when the files are written (the tick), and
says when the code gives none.

## What is not generated yet

Every plugin says in its notes what it leaves out. In this first version:
security and failure-containment loads (broken wire, unbalanced ice) —
use the broken-span solve and your own load cases; construction and
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
