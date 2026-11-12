# Radial clearances

*Built, in active development (not approved yet).* **Checks › Ground
clearances › Radial clearances…** in OHL-Design. This page explains the
window for a line designer: what it answers, how it works, every setting
and every field of a rule. The calculation in detail, with its sources:
[theory › Vertical and radial clearances](../theory/clearances.md). The
vertical window works the same way with a vertical distance in a zone
under the wire instead of the 3D one: [Vertical clearances](vertical-clearances.md).

## What it answers

**How close do the wires come to the ground and to what stands around
them — in any direction, below, beside or above — in every wind position
of the weather cases you name, and is that enough for your rules?** Each
span (and jumper) is compared with every rule that applies to it; a span
nearer than a rule allows is listed and marked in red in the views.

![Radial clearance: the shortest distance in any direction, the wire in still air and blown to the left and right, the swing band between the smallest and largest radius and its margin](../images/check-windows/2-radial-clearance-and-swing.svg)

*The radial clearance: the shortest distance from the wire in any
direction, in still air, blown to each side and anywhere on the swing
band between.*

## How it works, step by step

![The check in six steps: the weather cases, the wire positions, the distance in any direction, the ground and LiDAR points near the wire, the rule's distance compared with the distance found, the results and red marks](../images/check-windows/8-radial-check-step-by-step.svg)

### 1. The weather cases

Each rule names the weather cases it applies in (none named: every case of
the project). A case gives the temperature, the ice and the wind pressure;
the line is solved in it as in the rest of OHL-Design, each section by its
own calculation unless the rule or the window says otherwise. Only the
cases some rule names are solved.

### 2. The wire positions

In still air a wire hangs straight down from its attachments. With wind it
swings to the side. The window checks the wire in:

- **still air** and **blown to each side chosen** (left, right or both,
  looking ahead along the line) at the case's full pressure, always;
- the **swing arc** (on by default): every position between still air and
  the two blowouts. The span is swung as one about the line through its
  attachments, as a band from the smallest to the largest radius of the
  three solved positions, widened by the **band margin** (0.15 m). Three
  solves per windy case.
- the **wind steps** (off by default): still air and every pressure from
  0 up to the case's, in steps (50 Pa), each solved exactly. One solve per
  step and side.

![The swing band: the wire in still air and blown to each side, the band between the smallest and the largest radius, widened by the margin](../images/theory/swing-band.svg)

*The swing band between the blowouts, widened by its margin.*

Which to choose, with measured times:
[Arc or steps: speed and quality](../theory/clearances.md#arc-or-steps-speed-and-quality).

### 3. The distance in any direction

The radial window has **no zone to set**: it takes the **shortest 3D
distance** from the wire to each target, wherever the target lies —
below, beside or above the wire. A bundle is measured from each
subconductor. With the swing arc the distance is to the nearest place of
the band: inside the band 0, else the gap to its inner or outer radius or
to its end edges ([theory](../theory/clearances.md#the-radial-distance-to-the-arc)).

![Across the line: the vertical measured straight up within the zone's width; the radial, the shortest distance in any direction](../images/theory/vertical-radial.svg)

*The two measures side by side: the vertical window looks straight up
within its zone; the radial window in every direction.*

### 4. What is near: the ground, a TIN, the LiDAR points

A rule names its **targets**: the ground (the terrain surface of the
checks, or a TIN layer of the project by name) and/or **LiDAR classes** of
the point cloud (buildings, vegetation, roads…). The LiDAR points are read
again from the point cloud for each run, only those of the rules' classes
near the wires: up to the rules' largest distance and a margin (or the
distance set in *5 Model*). A wall beside the wire counts as soon as the
point cloud holds points of it — no "down to the ground" is needed here,
unlike the vertical window.

### 5. The rule's distance

The distance found is measured from the target to the **conductor's
surface** (each subconductor of a bundle) and compared with the rule's
distance at the wire's voltage: one distance, a table "up to kV:
distance", or a base plus an amount per kV. No distance is given for you:
take it from the regulation you work to.

![How a clearance rule is set and read: a rule table, the three forms of a distance evaluated at the line's voltages, and how a span is checked against a rule](../images/check-windows/6-how-a-rule-is-set.svg)

*A rule's distance in its three forms, and how one span is checked.*

### 6. The results and the red marks

The **Results** tab starts with the verdict: red, *✖ N spans below a rule —
worst margin …*, or green, *✔ Every span clear — worst margin …*. Below it,
one row per span below a rule, the worst first: the span, the rule, the
required and the found distance, the margin, the wind position (a pressure
and side, or *arc* and the angle on the swing arc), the target and how many
points are too close. A click on a row brings the views to the span, drawn
in its weather case and wind position. Each finding is drawn in **red** in
the 3D view, the cross-section, the profile and the plan while the window
is open: from the wire to the target. The notes under the results say what
was not checked or **not on the safe side**.

## The window, setting by setting

The Setup tab, top to bottom. Every setting is kept in the project's rule
file of the window (`<model>.radial-clearances.ohlr`) as soon as the
mouse is released, and a change marks the results *out of date*.

### 1 What it checks

The question in short, with **How it is computed** folded below it (the
method in a paragraph and the drawing of the 3D distance). **user guide ↗**
opens this page.

### 2 Rules

The rules, one row each. **Click a rule to edit it below**: the row is
highlighted and its fields appear under the list, titled
*Editing rule n: name*. A new rule (**Add rule**, **Duplicate**) is
opened there at once. Each field says on hover what it changes.

- **Preset ▾**: the rules of a design-code plugin or of the project; the
  chosen preset is shown first, then **Merge** (by name) or **Replace**
  (its rules, wind and what it solves). Its weather cases are matched to
  the project's ([Presets per check](design-codes.md#presets-per-check)).
- **Import…** / **Export…**: rule files (`.ohlr`).
- **Add rule**, **Duplicate**, **Remove** (asked first), **Move up**,
  **Move down**. The window always keeps one rule; a new window starts
  with **Ground**, its distance *to set*. **Run** is greyed out, with the
  reason, while a rule has no distance or no target.

The fields of a rule are in the [table below](#every-field-of-a-rule).

### 3 Wind

| Setting | Default | What changes in the result | Time |
|---|---|---|---|
| **swing arc** | on | every position between still air and the blowouts is checked: more findings, never fewer, than the blowouts alone | 3 solves per windy case (2 with one side), plus the band's lines for the ground; the window says the solves |
| **band margin** | 0.15 m | every distance to a span's band taken this much smaller (conservative); below 0.15 m the window warns: less safe | none |
| **jumper band margin** | 0 m | every distance to a jumper's band taken this much smaller. **At 0 m not on the safe side** between the blowouts (see [below](#what-is-conservative-and-what-is-not)) | none |
| **wind stepped** | off | still air and every pressure up to the case's solved exactly; between two steps nothing | one solve per step and side: the window says how many ("121 solves for a windy case of 600 Pa") and suggests the arc above 40 |
| **step** | 50 Pa | a smaller step finds the wire nearer its worst place between steps | more steps, more solves |
| **from** | both sides | left or right alone: only that side's blowouts (and band) | half the windy solves |

With the swing arc alone, a jumper band margin of 0 m and a rule that
checks jumpers, a warning under the wind says *NOT ON THE SAFE SIDE for
the jumpers …* before you run. Under the wind one line says that the
radial window has no zone to set (the distance is in 3D): the zone's
width and depth of the vertical window have no meaning here. The defaults
are the program's, unless the rule file states others (its `[wind]`
table; a `[wind]` without `step` takes the project's
`clearance_wind_step`). A rule file whose `[wind]` table has no `arc` key keeps the arc off: that is how
files written with the arc off read.

### 4 Scope

- **Circuits**: those checked (all ticked: every circuit, also those
  added later; none: Run greyed).
- **Routes** and their **alignments**: only the sections with a structure
  on one ticked (none ticked: the whole line). *View › Work on route*
  narrows it further.

Fewer circuits or routes: fewer sections solved, a faster run.

### 5 Model: more options (folded)

| Setting | Default | What changes | Time |
|---|---|---|---|
| **Solved by** | each section's calculation | every section by finite elements or by ruling span for this check (each keeps its cable condition) | finite elements are slower than ruling span |
| **Ignore LiDAR points farther than [m] from the wires** | off (as far as the rules reach) | points farther are not read nor compared; never a different clearance within the distance | fewer points read: faster |

### The footer

- **Use local conditions**: the line with its structures' local wind,
  snow and ice factors on the spans; said in the notes.
- Before **Run**, one line says what a run will do: the wind positions ×
  the sections, the rules, how far from the wires the widest rule counts
  points (its distance and the margin), the LiDAR points of the rules'
  classes and the **time limit** (900 s and 20 s more per million LiDAR
  points of the rules' classes).
- **Run** and **Stop**: the check runs in the background, each step with
  its own bar and time left; Stop keeps the previous results.
- **Where the time went** (under the results, folded): the measured times
  and plain hints.

## Every field of a rule

The window's rule editor and the rule file
([Clearance rules](terrain.md#clearance-rules-ohlr)) hold the same rule.
A radial rule in the file is a `[[rule]]` table with a `radial`
distance; the window writes one table per target kind (the points, the
ground) and reads them back as one row.

| In the window | In the rule file | Unit | Default | Meaning |
|---|---|---|---|---|
| Name | `name` | — | "Rule n" (the first: "Ground") | the rule's name in the results |
| Targets: terrain surface | `target = "ground"`, `surface` | — | the surface of the checks | the ground near the wire: none, the project's surface of the checks, or a TIN layer by name |
| Targets: LiDAR classes | `target = "points"`, `classes`, `roles` | class numbers | none | the points of these classes; `roles` take the classes of the class scheme with that role |
| Voltages | `voltages` | kV | every voltage | only the wires of these nominal voltages (0: earthed wires) |
| Weather cases | `cases` | names (`*` `?`) | every case | the cases the rule applies in |
| Calculation | `calculation` | e.g. `"max-sag-fe"` | each section's own | how every wire is solved for this rule (initial, after creep, after load, max sag; RS or FE) |
| Distance | `radial` | m | none (to set) | `"4 m"`, `{ by_voltage = [[110, "3 m"], …] }` or `{ base = …, per_kv = …, above_kv = … }` |
| Wires | `wires` | `"sections"`, `"jumpers"`, `"both"` | both | the sections' spans, the jumpers, or both |
| — (shown in the distance) | `altitude = { from, per_1000_m }` | m, share | none | the distance grows by this share per 1000 m above `from` |
| — (not used here) | `vertical`, `vertical_band`, `vertical_depth`, `to_ground` | — | — | the vertical window's distance and zone; the radial distance needs no zone and finds a wall beside the wire by its own points |
| — (not used here) | `screen` | — | — | a screen is a vertical rule: the radial window never writes it |
| — (not kept) | `exclude_near_ends`, `exclude_spans_at` | — | — | exclusion zones: the window says it does not keep them |
| — (not used here) | `search_margin`, `pair_voltage`, `include_same_phase`, `case_pairs`, `circuits` | — | — | for `ohl line` and rules between wires; a rule between wires is left out, said |

Read by the window from the whole file:

| Table | Key | Default | Meaning |
|---|---|---|---|
| `[wind]` | `steps` | false | **wind stepped** |
| `[wind]` | `step` | `"50 Pa"` | **step** |
| `[wind]` | `sides` | `"both"` | **from**: `both`, `left`, `right` |
| `[wind]` | `arc` | false when the table omits it (a new window: on) | **swing arc** |
| `[wind]` | `band_margin` | `"0.15 m"` | **band margin** |
| `[wind]` | `jumper_band_margin` | `"0 m"` | **jumper band margin** |
| `[check]` | `circuits`, `routes`, `alignments` | every circuit, the whole line | **Scope** |
| `[check]` | `method` | each section's own | **Solved by**: `"fe"`, `"rs"` |
| `[check]` | `search_distance` | none | **Ignore LiDAR points farther than** |
| `[check]` | `local` | false | **Use local conditions** |

The `[check]` keys `cases`, `sections`, `wind_study_min` and
`wind_study_cases` belong to other check windows; this window does not use
them.

## What is conservative and what is not

On the safe side (theory, [Wind positions](../theory/clearances.md#wind-positions)
and [The radial distance to the arc](../theory/clearances.md#the-radial-distance-to-the-arc)):

- the swing band holds every radius and every angle between still air and
  the blowouts, widened by the band margin of 0.15 m (the measured worst
  excess 0.089 m plus an allowance);
- on the band, a bundle is taken as its outer envelope, with the largest
  allowance of the three solved wires;
- the distance to the ground is taken from points along the wire at most
  0.5 m apart and lowered so that no place between them can be nearer
  ([Clearance](terrain.md#clearance));
- every distance is to the conductor's surface.

Not on the safe side, and said in the results each time it applies:

- **Jumpers with the swing arc, without the wind steps, at a jumper band
  margin of 0 m** (the default): a jumper's band does not hold the jumper
  between its blowouts (up to 0.293 m outside on the example line). The
  results then say *NOT ON THE SAFE SIDE: jumpers checked on their swing
  band without a margin …*, and the window says it under the setting.
  Enter a jumper band margin for your jumpers or tick the wind steps.
- **The structures rigid** in the wire solve: said in every result.
- The swing arc is a rigid swing about the attachments (a
  simplification; the band and its margin cover it on the example lines:
  [theory](../theory/clearances.md#wind-positions)).

The distance is to the points the point cloud holds: a part of an object
the scan did not see is not measured, the nearest point seen is.

## Without the window

`ohl zones --rules FILE --measure radial` runs the same check from the
command line on the window's rule file (`--measure` only for a file with
both distances) ([`ohl zones`](terrain.md#vertical-and-radial-clearances-without-the-window-ohl-zones)).
