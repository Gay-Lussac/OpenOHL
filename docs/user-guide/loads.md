# Load cases and structure loads (`.olslc`)

A **load case** combines a weather case, the wires' condition and load factors. The **structure loads** are the forces the wires and strings put on the structures at every attachment, in each load case: the starting point of the structure checks.

## Load case files (`.olslc`)

```toml
format = "ols-load-cases"
version = 1
name = "Synthetic example load cases"

[[case]]
name = "wind"
description = "high wind, factored"
weather = "high wind"      # a weather case of the model, by name
condition = "initial"      # initial, creep, load or max_sag
vertical = 1.0             # factor on the vertical loads
transverse = 1.4           # factor on the transverse loads
longitudinal = 1.0         # factor on the longitudinal loads
strength = 1.0             # strength (resistance) factor, for the capacity checks
tags = ["ULS"]
```

| Field | Required | Default | Notes |
|---|---|---|---|
| `name` | yes | | Unique in the file. |
| `description` | no | | E.g. the code clause it stands for. |
| `weather` | yes | | A weather case of the model's weather case file. |
| `condition` | no | `initial` | The wires' condition ([Calculations](line-file.md#calculations-condition-and-method)). After creep or after load needs the line's creep-stretch criteria. |
| `vertical`, `transverse`, `longitudinal` | no | 1 | Load factors, 0 to 100. |
| `strength` | no | 1 | Strength factor of the capacity checks; more than 0. |
| `tags` | no | | Code, limit state… |

Design codes (EN 50341 with a national annex, the Swiss LeV…) supply their combinations and partial factors as such files: a design-code plugin writes them from your parameter file ([Design codes](design-codes.md)). OLS CAD ships no code tables: the factors come from the standard you hold. `examples/synthetic-line/load-cases.olslc` only shows the format.

In a project, the load case files are kept in the project like the other libraries (Criteria › Load cases library edits them).

## Structure loads

Every section is solved in each load case with the finite elements (strings swing, each span its own tension), in the load case's condition and weather. At every attachment the load is the force the section puts on the structure: the reaction at the insulator top reversed (V-strings: one per leg), or at the clamp of a wire fixed to the structure. It is the whole bundle's load, and includes the strings' weight.

Each load is given:

- **in the line's axes:** x east, y north, z up, in N;
- **in the structure's axes:** transverse (along the structure's local x, to the right of the line looking ahead — `orientation_deg`), longitudinal (ahead), and vertical (down);
- **unfactored, and factored** by the load case's vertical, transverse and longitudinal factors.

A section that does not solve in a load case is listed, with why. Its loads are then missing.

**In the application:** Checks › **Compute structure loads** runs in the background (the model is not changed) and opens the **Structure loads** page. It shows one load case at a time (factored, or unfactored), a filter by structure, a structure's link to go to it, and **Export CSV…** for every case.

**From the command line:**

```bash
ols line line.ols.json --weather cases.olswc --load-cases loads.olslc --loads-csv loads.csv
```

The CSV columns: `case,structure,set,phase,label,support,x,y,z,transverse,longitudinal,vertical,factored_transverse,factored_longitudinal,factored_vertical`.

**Wind on the structures themselves:** a structure type may give the area the wind blows on, by height band (`[[wind.band]]`, see [Structure libraries](structure-library.md#wind-on-the-structure)). In every load case with wind, each band adds a load across the line — wind pressure × drag coefficient × area, on the wind side of the weather case — at the band's mid-height on the structure's centre line, listed as set 0 ("wind on the structure"), factored by the transverse factor. The capacity checks' load trees are per attachment and leave these out. Height extensions are not added to the bands yet.

Not yet included: jumper loads.

## Structure usage (capacity checks)

With the loads, OLS CAD checks how much of its capacity every structure uses, from its type's `[capacity]` (see [Structure libraries](structure-library.md#capacity)). Two methods, each used when the type gives its data:

- **Method 1, allowable spans.** The structure's **wind span** (half the sum of its adjacent spans, the largest over its conductor positions) and **weight span** (the wires' vertical load at the attachments, without the insulators, over their vertical load per metre in the load case — for a catenary, the distance between the low points of the adjacent spans, negative under uplift; the largest and the smallest over its conductor positions) against the type's allowable spans at the structure's **line angle** (the largest over the routes through it). The allowable spans are interpolated between the angles given; a structure turning more than the last angle fails ("not allowed"). Usage: the larger of wind span / allowable and weight span / allowable; a weight span below the minimum (uplift) is 100 % plus the shortfall over the allowable weight span. The strength factor does not apply: allowable spans are the type's design spans. A **conductor position** takes both sides of the structure whichever way they hang: one attachment carrying the back and ahead spans (a suspension point; a line end has only one side), or a dead-end's back and ahead strain strings — two attachments of the structure joined by a jumper whose two ends both end a section there. A jumper from a branch's end to a point in another section's middle (a tee-off) joins nothing: the branch is a position of its own. Without the jumper, a dead-end's two strings are checked as two line ends.
- **Method 2, allowable loads (load trees).** At every attachment the type gives, the **factored** loads in the structure's axes (summed over a V-string's legs) against the allowable loads times the load case's **strength factor**: transverse and longitudinal either way, vertical downwards, uplift upwards (an upward load with no uplift given fails). Usage: the largest ratio of the attachment, or their sum with `interaction = "linear"`; the structure's is its worst attachment's.

Two more checks come from an analysis, when the type names a structure model or a pole (see [Structure analysis](structure-analysis.md)):

- **fe** — the type's finite-element model (`model = "….olsfe"`) solved with the factored attachment loads; usage: the member using most of its capacity.
- **pole** — the type's pole (`pole = "id"`): bending moment along the pole from the factored loads and the wind on the pole, P-Delta amplified, against its moment capacity.

A structure's usage is the largest of its checks. A structure without a type, or whose type gives nothing to check against, is listed as not checked. A load case with sections that did not solve misses their loads: it is said so.

**In the application:** on the Structure loads page, **Usage** shows every structure worst first — for the governing load case of each (or one load case): usage (red over 100 %, orange from 90 %), each method's usage (spans, loads, fe, pole), what governs (e.g. "weight span 640 m of 600 m", "1:2 transverse 31.2 kN of 30.0 kN × 0.9"), the load case, line angle, wind and weight spans — and **Export CSV…**.

**From the command line:** the loads run prints the ten structures using most of their capacity; `--usage-csv usage.csv` writes every load case and structure:

```bash
ols line line.ols.json --weather cases.olswc --load-cases loads.olslc --usage-csv usage.csv
```

The CSV columns: `case,structure,type,line_angle_deg,wind_span,weight_span_min,weight_span_max,spans_usage,loads_usage,fe_usage,pole_usage,usage,governs,notes` (usages in %).

