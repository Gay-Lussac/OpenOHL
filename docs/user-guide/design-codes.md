# Design codes

A **design-code plugin** turns your **parameter file** into weather cases
([`.olswc`](weather.md)) and load cases ([`.olslc`](loads.md)) for a line:
the cases the code asks for, the wind pressures and ice loads computed
the way the code computes them, the partial and combination factors put
on the vertical, transverse and longitudinal loads, and the strength
factor. The plugin also returns **notes**: what it computed, assumed or
left out.

**OLS CAD ships no code tables.** Standards are copyrighted, and their
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
| `lev-ch` | Swiss Leitungsverordnung (LeV / OLEI, SR 734.31) | SI |
| `iec60826` | IEC 60826 (reliability-based) | SI |
| `nesc` | NESC (ANSI C2) Rule 250 | US customary |
| `csa-c22-3` | CSA C22.3 No. 1 | SI |
| `asce-mop74` | ASCE Manual of Practice 74 | SI |
| `go95` | CPUC General Order 95 (Rules 43, 44) | US customary |

Build them once (they are WebAssembly components):

```bash
rustup target add wasm32-wasip2
for d in plugins/codes/*/; do [ -f "$d/ols-plugin.toml" ] && (cd "$d" && cargo build --release --target wasm32-wasip2); done
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

### Swiss LeV

The additional (ice / snow) load per metre is base + per-mm × the
reference conductor diameter, from the altitude zone of the site; wind,
the reduced combination of both and a minimum temperature case follow.
Loads are not factored; the strength factor is 1 / safety factor.

## From the command line

```bash
ols code list                               # plugins/codes, or --dir DIR
ols code template --plugin plugins/codes/en50341 --out my-line.toml
# … fill in my-line.toml from the standard …
ols code generate --plugin plugins/codes/en50341 --params my-line.toml \
    --weather my-line.olswc --load-cases my-line.olslc
ols line line.ols.json --weather my-line.olswc --load-cases my-line.olslc --loads-csv loads.csv
```

`generate` checks both files as OLS CAD reads them (and that each load
case names one of the weather cases) before writing them, and prints the
notes.

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
load cases. The code's notes are shown in the window.

## What is not generated yet

Every plugin says in its notes what it leaves out. In this first version:
security and failure-containment loads (broken wire, unbalanced ice) —
use the broken-span solve and your own load cases; construction and
maintenance loads; wind on structures by the code's own method (the
structure loads apply each weather case's wind pressure to the structure
types' height bands, see [Load cases](loads.md)); the NESC constant k (the notes give
its value to add by hand); per-wire ice loads that depend on the conductor
diameter (LeV: one reference diameter per parameter file).
