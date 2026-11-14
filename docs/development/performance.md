# Performance and time budgets

## Benchmarks

Performance-sensitive code has [criterion](https://github.com/bheisler/criterion.rs) benchmarks in `crates/<crate>/benches/`:

```bash
cargo bench --workspace --bench '*'
```

The CI workflow is started by hand (GitHub Actions › CI › Run workflow); with **full** ticked it also runs the benchmarks: it benchmarks the parent commit on the same machine first and fails if a benchmark becomes more than 1.5× slower. Shared CI machines are noisy, so the threshold is deliberately loose; local runs give precise numbers.

Example: `units/total_load_typed_10k` vs `units/total_load_raw_10k` check that typed quantities cost nothing compared with plain `f64`.

`drawn/bodies_load_example` and `drawn/place_lattice_tower_100x` (`ohl-structures-fe`) time the structure bodies drawn at levels 2 and 3: made once per type and extension, then only moved to each placed structure (a lattice tower of 178 members with their solids, placed 100 times). The app test `member_tests::many_lattice_towers_stay_fast` builds the structures layer of 400 such lattice towers under a lenient 5 s, debug builds included.

### Member and pole solids

Solids of members with a library profile and of poles at levels 2 and 3 are drawn in the chunks within the **solids distance** (`lod::LodSettings::solids`, its own value in the Level of detail page, default `lod::DEFAULT_SOLIDS_DISTANCE` = **400 m**), apart from the structures' level 0 (2 km), which keeps drawing the members as lines. The benchmark `member_tests::solids_distance_benchmark` measures it on 400 type-02 lattice towers 400 m apart with every member a library angle (178 solids of 20 triangles, 3 560 triangles a lattice tower, 1.4 million in all), with the frame benchmark's harness:

```bash
OHL_REQUIRE_GPU_TESTS=1 cargo test --release -p ohl-app solids_distance_benchmark -- --ignored --nocapture
```

Apple M-series, release, 1600 × 1000 window, median frame (other builds running on the machine, so the slowest frames are noise):

| Solids distance | Down the line from 80 m behind a structure | from 600 m behind | 80 m from a structure, oblique | 600 m, oblique | whole line |
|---|---|---|---|---|---|
| 0 | 0.43 ms, 0 triangles | 0.28 ms, 0 | 0.45 ms, 0 | 0.24 ms, 0 | 0.28 ms, 0 |
| 200 m | 0.25 ms, 3 560 | 0.32 ms, 3 560 | 0.24 ms, 3 560 | 0.24 ms, 0 | 0.28 ms, 0 |
| 400 m | 0.29 ms, 3 560 | 0.44 ms, 3 560 | 0.26 ms, 3 560 | 0.24 ms, 0 | 0.40 ms, 0 |
| 800 m | 0.28 ms, 7 120 | 0.36 ms, 7 120 | 0.29 ms, 3 560 | 0.25 ms, 3 560 | 0.24 ms, 0 |
| 1 200 m | 0.27 ms, 10 680 | 0.28 ms, 10 680 | 0.24 ms, 3 560 | 0.26 ms, 7 120 | 0.24 ms, 0 |
| 2 000 m | 0.49 ms, 17 800 | 0.38 ms, 17 800 | 0.24 ms, 3 560 | 0.24 ms, 7 120 | 0.32 ms, 0 |

Every view settled in 4 frames. The frame time does not depend on the distance here: a few lattice towers' solids are well within the 16 ms of a 60 Hz frame (the lines and the rest of the view cost the same at every distance). So the default is set by **what can be seen**, not by the frame time: at 1000 px for a 45° field of view a metre at distance *d* covers about 1200 / *d* px, so a single 60–100 mm angle is under a pixel beyond some 70–120 m (a lattice tower's many members together still read as a lit mass a few hundred metres off) and a pole of 300–700 mm is 1–2 px at 400 m. Beyond that the solids only add triangles, GPU memory and chunk builds (their number grows with the distance: 5 lattice towers at 2 km down a line of 400 m spans, many more on a line with short spans or several circuits side by side) for nothing seen, the lines drawn over them giving the shape. 400 m keeps the structure in front and its neighbour at usual spans (300–400 m) solid, where the profile and the pole's taper can be seen; the user raises it in the Level of detail page for close-up work along a line, or sets 0 for none. Display only: no computed number depends on it.

### DXF drawings

A drawing is indexed once into the 256 m chunks (lines cut where they cross tiles) and each chunk built at its level on the GPU when the view needs it, like the other chunked layers (`chunks::ChunkIndex`, `ohl_render::lod::ChunkSet`); its texts are written in the plan only. The benchmark `bench::drawing_benchmark` adds an invented drawing (`bench::large_drawing`) to the synthetic example — 28 km along the line, parcels of 40 m with a number each, 300 buildings, 600 boundary marks (circles of 73 points) and 300 points a km², contour lines 30 m apart with a vertex every 2 m — as a 1.2 km corridor (51 000 lines, 2.2 million vertices, 31 000 texts) and as a 9 km wide sheet (385 000 lines, 16.8 million vertices, 233 000 texts), and flies 10 km along it (the frame timed until the GPU has drawn it):

```bash
OHL_REQUIRE_GPU_TESTS=1 cargo test --release -p ohl-app drawing_benchmark -- --ignored --nocapture
```

Apple M-series, release, 1600 × 1000 window, 9 km sheet (median frame; segments drawn; what the chunks hold on the GPU after the flight):

| Drawings row | 600 m from a structure | 80 m from a structure | flying 50 m a frame, 400 m behind | flying 100 m a frame, 2.5 km behind |
|---|---|---|---|---|
| former default (level 1 from 1 km at 0.5 m, level 2 from 4 km at 2 m) | 0.72 ms, 214 000 | 0.68 ms, 194 000 | 4.08 ms, 337 000, 61 MB | 4.71 ms, 268 000, 48 MB |
| default (300 m at 0.2 m, 2 km at 1.5 m) | 0.66 ms, 143 000 | 0.64 ms, 138 000 | 3.73 ms, 274 000, 40 MB | 4.30 ms, 260 000, 23 MB |

Indexing the sheet takes 0.4 s (0.05 s the corridor), building every chunk at level 0 0.15 s (0.04 ms a chunk), at level 1 0.3 s; the chunk plan 0.15 ms a frame. The CPU's share of a frame stays under 2 ms; the rest is the GPU drawing the segments, about 15 ns each, so the frame time follows the segments drawn. The new defaults err by about a pixel where each level begins (1200 / *d* px a metre: 0.2 m is 0.8 px at 300 m, 1.5 m 0.9 px at 2 km) and draw a fifth to a third fewer segments; a boundary mark of 73 points becomes 5 beyond 300 m. An edit of the line no longer indexes the drawings again (it did, with every built chunk dropped: 0.4 s for the sheet, then the chunks built again), nor does showing or hiding the whole layer.

### Sections in the blocks over sheaves

Rolling over frictionless sheaves (the default since 2026-10-25; [Sag-tension](../theory/sagtension.md#sagging-outputs)) costs more than one horizontal tension: the equal-H sharing first (its tension the start), then one unknown searched with every span's tension found from the sheave before it — about 10–20 outer steps, each a few element solves per span (the span's last length the start of a secant on its length, else the bracketed search). Measured on the synthetic example (`ohl line`, one weather case, debug build): 0.42 s over sheaves against 0.26 s with equal H; the secant start cut it from 0.76 s with the same results. Exit clauses: the solver's budget on the first span's tension, 200 steps per span, 20 settling passes of the strings. A riser (a jumper given by its tension) is one wire solve per weather case of 8–64 bars (0.1 s in a debug test for a 30 m riser), its cut length 5–15 of them once.

## LiDAR clearance checks (2026-10-27)

The owner's vertical run on a real project (two combined lines, about
170 structures and 350 sections, 71 million LiDAR points, a design
code's preset with every clearance check, 31 wind positions) did not
finish in 900 s; the radial one (53 wind positions) neither in the
desktop application. Measured on a read-only copy of that project with
`ohl zones` (10 cores):

- **Cause (vertical).** The preset's *screen* rules require 1000 m
  vertically (`SCREEN_DISTANCE`: every point within their band in plan
  is reported, at any height). That 1000 m became the reach of the wire
  index: each wire segment was listed in every 10 m cell within 1000 m
  — some 40 000 cells per segment, on one core, outside the time limit's
  checks. The run was stopped after 13 minutes in that loop at 5.4 GB,
  its 400 s limit ignored. Now each wire is listed only as far in plan
  as a point can count for its check (`checks::plan_reach`: the band of
  a vertical check, a screen's by voltage; the radial reach and the
  outer radius of a radial one; the same for the swing arcs); a screen
  in the radial window no longer carries its 1000 m.
- **Then: reading.** The test whether a decoded point lies in the
  corridor around the wires (every wire in every wind position) ran on
  the visiting thread alone: 204 s for 20.6 million points. It now runs
  on the decoding cores: 36 s.
- **Then: indexing** a preset's 49 rules × 17 000 wires one after
  another: 27 s; now on every core: 1–8 s.
- Every rule and wind position is checked against each block of 65 536
  points in one pass (one set of threads per block, not one per rule).

Real project, the same results (CSV identical): vertical, did not finish
(stopped after 13 min) → **53 s** (2.8 s solving, 1.3 s indexing, 36 s
reading, 2.9 s checking); radial, 4 min 01 s → **1 min 50 s – 2 min 30 s**
(reading 50–100 s, varying with the machine's load).

Synthetic (`zone_speed`, `SPANS=600 POINTS=3000000 STEP=50`, bench-fast,
10 cores; 180 sections, 27 wind positions), before → after:

| Check | Before | After |
|---|---|---|
| vertical, screen rule (`SCREEN=1`) | not finished after 16 min (stopped) | 2.5 s |
| vertical, all rules with a screen (`SCREEN=1`) | not finished | 3.6 s |
| vertical, vegetation (no screen) | 3.5–5.5 s | 1.5–2.1 s |
| vertical, all rules (no screen) | 4.9 s | 2.3 s |
| radial, vegetation (no screen) | 5.6 s | 5.2–6.6 s |
| radial, ground on the swing arc | 38 s | 34–43 s |

Without a screen the checks were already bounded: the change is within
the noise of a loaded machine, apart from the one pass. The ground on
the swing arc (radial, 3 solves) is the slowest synthetic part left; the
reading of the corridor (a test per point against every wire position
near it) is the slowest part of a real run. The tests
`narrow_cells_give_the_results_of_before` and
`a_case_check_with_narrow_cells_gives_the_results_of_before` compare the
results with the cells of before, bit for bit.

**Time limit.** 900 s and 20 s per million LiDAR points of the rules'
classes in the cloud (`zone::time_limit`): about ten times the ~2 s per
million measured on the real project. Shown before Run and in the error
when reached.

## Radial clearances: every step counted, Stop, the corridor test (2026-11-04)

The owner's radial run on a real line (1.1 GB project, 511 sections, a
design code's 36 radial rules, the swing arc in 3 weather cases, 7
wind positions, search distance 30 m) said "about 0 s left" after some
800 s and went on; Stop did nothing. Measured on a copy with the
project's own rule files (`bench::clearance_zones_benchmark`, below;
bench-fast, 10 cores, other builds running on the machine):

- **The progress counted the points read only.** Once they were read
  (100 %), the ground check of the swing arcs — 1342 arcs, 3.0 million
  lines of their boundaries at the 2 cm tolerance — ran for minutes
  uncounted. Now every step is counted with its own bar and time left
  (`ohl_clearance::zone_progress`): solving the positions, making the
  arcs, indexing the wires, reading and checking the points (from the
  point reader's counts, reset for each read), checking the ground (each
  wire position and each line of each arc). No time left is said before
  2 % of a step nor once its counter is full.
- **Stop.** The points were checked in blocks of 65 536 without looking
  at the token, and the ground wires and arcs not at all: Stop waited for
  the end. Now the check looks every 2048 points on each core, before
  each wire and each arc line, each span while making the arcs and each
  rule while indexing; the window frees itself at once (the previous
  results kept) while the thread winds down. Test:
  `a_zone_check_stopped_among_the_points_returns_at_once` (40 million
  points offered, stopped after 0.1 s of checking: returns within 5 s).
- **Reading: the corridor test.** Each point read is tested against the
  corridor around every wire position and every swing arc (the arcs
  give a polyline every metre of their swing: many polylines). The
  vertical window has no swing arc, so its corridor was cheap; with the
  arcs the test took most of the radial run (262 s of 514). A chunk's
  part of the corridor now has a fine grid (cells of a quarter of the
  width): a cell wholly inside (its four corners within the width of one
  segment, less 1 µm: the distance to a segment is convex), wholly
  outside (no segment within reach, by a lower bound from its centre) or
  the segments that may reach it, nearest first; the parts are made on
  every core. The same points are read (`a_local_corridor_answers_as_the_whole`
  compares with the test against every segment, at the edge too).
- **What the vertical had that the radial lacked:** nothing in the
  checking itself — both run the same one pass over the points (every
  rule, wind position and arc against each block on every core, since
  2026-10-27). The radial differs by its swing arcs: their polylines in
  the corridor (fixed above) and their ground check (below).

| Real line (findings identical, compared in full) | Before | After |
|---|---|---|
| radial, in all | 514 s | 225 s |
| radial, solving 7 positions | 35 s | 34 s |
| radial, reading the points (10.3 M near the wires) | 262 s | 9 s |
| radial, checking the points | 5 s | 3 s |
| radial, the ground with 1342 swing arcs | 205 s | 173 s (noise) |
| vertical, in all | 69 s | 45 s |
| vertical, solving 30 positions | 42 s | 41 s |
| vertical, reading the points (9.7 M) | 24 s | 1 s |

**Left as it is:** the ground check of the swing arcs (some 3 minutes
here) checks each arc's boundary as lines 2 × `ARC_TOLERANCE` = 4 cm
apart (the swing arc's precision), each as a wire against the ground
triangles. Two exact speed-ups were tried and dropped: a nearest
triangle found ring by ring (no gain: the nearest is about as far as the
search radius, so nearly the whole box is searched anyway) and
passing over samples by the distances of the neighbouring lines (the
distance to a surface changes by at most the distance moved; no gain:
the searches are cut at the rule's reach, below the clearances to beat,
so the bounds never sufficed). A coarser tolerance would be faster but
coarsens computed precision: the owner's decision. The solving of the
wind positions waits for its slowest position (one whole-line solve of
some 25 s here).

The benchmark (a copy of a project of your own: it is opened for
editing; `OHL_BENCH_OUT` writes the findings in full to compare runs):

```bash
OHL_BENCH_FILE=copy.ohl OHL_BENCH_OUT=out/ OHL_BENCH_MEASURE=radial \
  cargo test --profile bench-fast -p ohl-app clearance_zones_benchmark -- --ignored --nocapture
```

## Frame benchmark (desktop application)

The desktop window is drawn headless (egui_kittest with wgpu, as the GPU tests) with the 3D view at four distances (the whole line, 3 km, 600 m and 80 m from the middle structure) and the plan view at four zooms (the whole line, 3 km, 400 m and 60 m wide). For each view it settles (every chunk built, every point cloud node read), checks that an idle frame asks for no other one, then draws 20 frames while turning or panning a little (as a drag does) and reports the median and slowest frame, the time to tessellate the frame's shapes, the shapes, what the 3D view drew, the time of a click (picking) and the time of each phase (`crate::perf` spans: frame hooks, menus, each tab, 3D layers rebuilt, chunk plan and build, point stream, render, labels; plan wires and labels; …):

```bash
OHL_REQUIRE_GPU_TESTS=1 cargo test --release -p ohl-app frame_benchmark -- --ignored --nocapture
# a project of your own (work on a copy: it is opened for editing), with
# your view preferences (a ViewPrefs JSON) and frames per view:
OHL_BENCH_FILE=copy.ohl OHL_BENCH_PREFS=prefs.json OHL_BENCH_FRAMES=40 cargo test --release …
```

Phases are timed only while the benchmark turns timing on (`perf::enable`); otherwise a span costs one atomic load. The tests `idle_frames_neither_repaint_nor_rebuild` (no repaint asked, no chunk built and no 3D image drawn while nothing changes; a camera move draws one image), `frames_stay_within_a_lenient_budget` (every view of the synthetic example under 250 ms, debug builds included) and `structure_names_show_from_afar` keep it so.

Synthetic example (`examples/synthetic-line`, Apple M-series, release, 1600 × 1000 window, median frame of the app's drawing / tessellation):

| View | Before | After |
|---|---|---|
| 3D whole line | 0.30 / 0.07 ms | 0.24 / 0.07 ms |
| 3D 3 km | 0.34 / 0.09 ms | 0.22 / 0.07 ms |
| 3D 600 m | 0.33 / 0.08 ms | 0.22 / 0.06 ms |
| 3D 80 m | 0.31 / 0.08 ms | 0.23 / 0.06 ms |
| plan whole line | 0.18 / 0.08 ms | 0.14 / 0.06 ms |
| plan 60 m | 0.17 / 0.06 ms | 0.14 / 0.06 ms |

The example is small; on a real project of a few hundred spans with a corridor survey (private data, not in the repository) the same views went from 5.7–6.4 ms (3D) and 55–77 ms (plan) per frame, plus 11–16 ms tessellating some 60 000 shapes, to 0.7–0.8 ms (3D) and 1.1–1.8 ms (plan) plus about 1 ms tessellating 5 000–10 000 shapes. What it found:

- **Plan**: every section's shapes copied every frame (`Model::shapes_shown`, now `shapes_shown_ref` borrowed), the wire styles hashed once per jumper, and every span drawn, also off the screen. Now only what is in view is drawn (`plan::Cull`), the points a far view crowds into one pixel once (`plan::thin_px`), sags and section labels made only for spans in view long enough on the screen (`annotations::notes_where`), the alignment views kept while the alignments and structures do not change (`plan::AlignmentViews`).
- **Profile** (shown under the 3D view in the default layout, so drawn with every frame): up to 200 000 survey points drawn one circle each and the whole route's ground lines with every point. Now one dot per pixel (`profile::PixelSet`), lines thinned to half a pixel, the corridor's and the wires' shapes and the heights seen kept while the view, the selection and the styles do not change.
- **Every frame**: a closed window copied the whole line (`alignment_windows`); the section labels' search was quadratic in the sections.
- **3D**: the image is drawn again only when the camera, the size or the scenes on the GPU change (`View3d::image_key`): frames for another panel (a mouse move over a table) cost the GPU nothing.

Picking (a click) stays exact: it uses the full model, not what the levels draw (some 6 ms on the real project, 0.05 ms on the example).

**2026-10-22, a real project with lattice towers and two DXF drawings** (private data; 76 structures of thousands of members each, 45 000 drawing entities; the same benchmark): the 3D view's frames took 7 ms plus 14 ms tessellating 175 000 shapes, a click 3.1 s. Found and changed:

- **Click (picking)**: the rigid links' candidates listed every placed structure's joints, each joint looked up by building the type's whole joint list again (quadratic in the joints: a lattice type has thousands). Now `rigid_links` places no joints for a structure without a linked point at an offset (the usual case), `placed_joints` lists the joints once, and `StructureType::joints` finds the linked ones by a set. 3.1 s → 25 ms.
- **Profile** (under the 3D view, drawn every frame): every structure of the route drawn with every member, off the screen too. Now only those within the stretch shown; under 80 px tall on the screen, a structure of more than 40 bars is drawn as its mast and an arm out to each point (its bars blur into one shape there; display only); the corridor's dots one mesh instead of a circle each.
- Result per frame, 3D view: 7 ms + 14 ms tessellating 175 000 shapes → 0.9–1.0 ms + 1.7 ms, 8 000 shapes; plan view: 7.5–9.1 ms + 15–17 ms → 1.4–3.1 ms + 1.9–4.2 ms.

### What is drawn at which distance

Display only: calculations, checks and picking always use the full model; nothing here changes a computed number (`ohl validate benchmarks` and the CLI's wire, span, load and usage outputs are identical before and after these changes). The distances of each layer's levels are the Level of detail table's (per user; defaults in `lod::LodRow::default_for`), what each level keeps is fixed (`scene::structures`, `scene::wires`, constants `lod::DASH_SCALE` and `lod::BODY_DOT_PX`):

| Layer | Level 0 (default to) | Level 1 (to) | Level 2 (to the render distance) |
|---|---|---|---|
| Terrain, other surfaces | every TIN triangle (1.5 km) | 8 m grid (5 km) | 32 m grid (30 km) |
| LiDAR | streamed by node under the point budget (5 million) and the error on the screen (1.5 px), to 3 km | | |
| Structures | members of levels 2 and 3 (else the stick drawing), arms, strings (2 km); members with a library profile and poles at levels 2 and 3 also as solids within the solids distance (400 m, its own setting) | shaft line, arms, strings (8 km) | shaft line (50 km) |
| Wires, other cases, project lines | subconductors and yokes, clamps and bodies to their shape, bare points' marks, jumpers' support clamps, line codes in 1.5 m units (1.5 km) | one line per bundle, 16 points per span, dashes × 4, bodies as 4 px dots, spreads, spacers, arrows (6 km) | 5 points per span, dashes × 16 (50 km) |
| Sections not solved | dashed red, 1.5 m units | dashes × 4 | dashes × 16 |
| Drawings (DXF) | every vertex (300 m) | simplified to 0.2 m (2 km) | 1.5 m (10 km) |
| 3D labels | sags, section labels, points', strings', wire points' labels within the label distance (1.5 km) | | structures' names at any distance, placed first |

Plan: what is in view, half-pixel thinning, structures' names at every zoom (overlapping ones left out, the first in line order and the selected one kept). Profile: one survey dot per pixel (all dots one mesh), half-pixel thinning, shapes kept while the view stands still; only the structures within the stretch shown, those under 80 px tall with more than 40 bars as their mast and arms.

### Repaints

An idle window asks for no frame. Only these ask for frames while they run: a camera key held (every frame), chunks or point cloud nodes still to build or read (every frame, 50 ms), a structure file open in the structure application (its file looked at twice a second), solving in the background, jobs and servers (their own intervals).

### Tables page

The rows of a table are kept while what they are built from is the same (`tables::TablePage::cached_rows`, 2026-10-27; before, built every frame): the table and weather case, the line's version (every edit counts it up), the library files' revision, the ground, each weather case's solution (`Solved::serial`, counted up whenever a case is made or solved again), the cases sections are drawn in, the model's files, the plugin columns and whether the project can be edited. Display only: the rows are the same either way (a test compares them with rows built anew after an edit, a solve and another case). The page's frames, headless without a renderer:

```bash
cargo test --release -p ohl-app --bin ohl-design tables_frame_benchmark -- --ignored --nocapture
```

Synthetic example (Apple M-series, release, median of 60 frames of the page alone, before → after): Structures 0.142 → 0.133 ms, Sections 0.129 → 0.118, Strings 0.302 → 0.287, Conductor paths 0.084 → 0.054, Circuits 0.062 → 0.049, Routes 0.051 → 0.039, Alignment points 0.110 → 0.098, Weather cases 0.134 → 0.125, Point and spread load types 0.217 → 0.193 ms (that table read the project's library files from the project file every frame). The example has a handful of rows per table, so what remains is egui drawing the cells; the rows' cost grows with the line (every structure's sections counted, the routes, the circuits found), and is now paid once per change.

### Check windows: result rows and choice lists (2026-11-10)

What a check window worked out again on every frame while it was shown,
and what it is keyed by now. Display only: the rows, their order, texts
and numbers are the same (tests compare kept rows with rows made for the
frame, in several views).

- **The result rows.** Each window built a fresh row per result row,
  every cell written to a text, on every frame. A window now keeps them
  (`check_ui::CachedRows`) and builds them when their key changes:

  | Window | The rows are built again when … changes |
  |---|---|
  | Wire tensions, String checks, Internal clearances, Lightning, Ice shedding, Galloping | the count of answers taken (`Job::generation`; a saved run shown counts), the number of result rows |
  | Phase distance, Unequal wind | the same, and whether a comparison with a saved report is shown (as since 2026-11-04) |
  | Vegetation, Safety volumes | the count of answers taken, the window's count of reports (a saved run shown), the number of spans or volumes |
  | Vertical and radial clearances | the count of reports taken, the number of findings, the point classes' numbers and names (a hash; the Target column says them) |
  | Clearances | the count of reports taken, the number of findings |
  | Structure loads | the count of answers taken, the load case shown, factored or unfactored, the structure filter |
  | Thermal rating | Ampacity, Temperature: the key their values are kept by already; Step change: that key, the print interval and the maximum temperature; Clearance limit: the count of runs arrived, the line's version, the method, the range and the ambient cases; At the current: the count of runs arrived and the cases' names |
  | Buildings (fields) | the count of answers taken, the preset shown |
  | Bill of materials | the count of answers taken, the number of lines |

  *Only violations*, the search, the sort and the grouping are not in the
  keys: they choose among the rows. A result taken away shows no table;
  the next answer has another count.
- **Filtering, sorting and grouping** (`ResultTable`): over all rows on
  every frame before, the search lower-casing every cell. For kept rows
  the answer (the rows shown in order, the group headings, the count of
  violations) is kept per table in the session's memory and made again
  when the rows (every build has a stamp of its own), *Only violations*,
  the search, the sort column and direction, or the grouping change.
- **Safety volumes**: each mesh's volume, a sum over all its triangles,
  was worked out for the table on every frame; now once with the rows.
- **The rule editors' lists** (Wire tensions, String checks, Internal
  clearances, Ice shedding, Lightning): the weather cases' names, the
  wire types (sorted, each once), the systems, circuits and voltages, and
  for Lightning what is on the route worked on, were made from the line on
  every frame. `check_ui::LineChoices` makes them once per version of the
  line (`Model::line_version`), route worked on and case names (the names
  themselves are compared). The weather case groups are shared instead
  of copied, and read when the libraries change (below; until then their
  file's modification time was asked for on every frame the Setup tab was
  shown).

Measured (a timing of the table's work alone, without egui drawing the
cells; Apple M-series, release, median of 40):

```bash
cargo test --release -p ohl-app rows_kept_timing -- --ignored --nocapture
```

5000 rows of seven cells, written as Wire tensions writes them. Before,
every frame: the rows built, then filtered, sorted and grouped. Now: the
first frame after a run or a change of view the same work, the frames
after it a comparison of the keys.

| View | Before, every frame | Now, first frame | Now, the frames after |
|---|---|---|---|
| Only violations (the default) | 3.13 ms | 3.19 ms | 0.0003 ms |
| Every row, sorted by tension, grouped by rule | 3.38 ms | 3.41 ms | 0.0003 ms |

Not measured: the whole frame of a window (drawing the rows in view is
unchanged), the safety volumes' mesh volumes, the rule editors' lists.

### What else was recomputed every frame (2026-11-10)

Found by a review of the frame's work and removed the same day. Only
*when* something is worked out changed: the values, texts and drawings
are the same (each item has a test that compares with the computation as
it was, or counts how often it is made). Every kept value names all it is
made from in its key; `Model::line_version` counts every change of the
line (the line is never changed in place), `Solved::serial` every solve of
a weather case, `Model::revisions.libraries` every reading of the library
files. Most of them are kept by the model (`model::ModelKept`, each a
`kept::Kept`: the last value and its key), so a model opened anew starts
with nothing kept. Not measured: the list below is what was done on every
frame and what it is keyed by now.

| What | Every frame before | Now made when … changes |
|---|---|---|
| Weather case groups (the checks' Setup tabs, `case_groups::groups_shared`) | a `stat` of the weather case file | the file used (its path) or the libraries' revision; at once when the model reads its libraries (also when they then do not load) and when the Weather case groups window is opened. A file changed outside the program shows at the next of these (the owner's decision) |
| Phase distance, *Values used* | the values file read from disk (a file chosen), or every library file's text copied to find the project's copy, then two more copies of the text | a file chosen: the file, its modification time or its length (`phase_distance::KeptFile`; one `stat` per frame is left). The project's copy is still asked of the project every frame, now without copying the other files (the next row); the copies of the text are gone. A run reads the values itself, as before |
| A library page's *unsaved* (`LibraryPage::dirty`: every Library tab's title, hidden ones too, its buttons; the load case form's text; a wire's graph) | the whole document written out as text and compared, up to five times a frame per page | the document: its text and the answer are kept (`library::edited::Edited`) until the page replaces or changes it — the page can reach the document only through `open`, `replace`, `edit` and `edit_if` (an editor shown every frame, which answers whether it changed anything: what the page already checked the file on) |
| `Session::library_files(kind)` | kind, name and text copied for every library object, then all but one kind thrown away (asked for each of the 15 kinds in turn) | nothing kept: only the library objects are gone through, the kind compared first |
| Profile: section labels and sags | a note for every span of the line (its points copied), all of them looked through for every trace | nothing kept: only the notes of the traces written on are made, found by section and span (`profile::span_notes`); traces more than 2000 px (or the view's width) beyond the view's sides get none |
| Plan: the objects' labels | the structures' names made (every member point of every structure looked at for its top) and thrown away | nothing kept: not made (`annotations::object_notes_unnamed`) |
| 3D: the structures' names | every member point of every structure looked at for its top | the line's version (`annotations::structure_tops`) |
| Where the strings are drawn (`selection::string_pieces`: the labels, picking, and the selection's highlight — once per selected object and case) | a map of every string of the line, each section searched for by id | per weather case: the line's version and the serials of the case's solution and of its other wind side (`selection::KeptPieces`) |
| Properties of a structure: its routes | the routes of every structure worked out, twice | the line's version (`Model::routes_of`, as the other panels) |
| The alignments with their structures (`Line::alignment_views`) | worked out by the open Profile sheets, Plans and Spotting windows; hashed from every alignment, point and structure by the plan, the profile and the cross-sections, the profile copying the result | the line's version, one shared list (`Model::alignment_views`) |
| The shapes shown (`Model::shapes_shown_ref`, the plan) | each section searched for in every case's shapes (the square of their number) | nothing kept: each case's shapes indexed by section once per call. The other project lines in the plan: the cases shown asked once, each section's once |
| Lines on the ground: whether they are still right (`ground_lines::key`) | every clamp, low point and mid-point of every solution displayed hashed; every section asked for its cases | each solution's serial instead of its points (a case solved again to the same shapes makes the lines once more: the same lines); only the sections that can have cases of their own are asked |
| Structure editor page, the windows of a deletion of an alignment point or a spread, the route choice | the whole line copied (the editor page: twice) | nothing kept: read where it is; copied only by the action that changes the model while reading it |
| Which plugins are in use (`Plugins::enabled`, asked for each plugin by five lists a frame) | each plugin's entry file looked for on disk each time | the plugins listed (each folder, duplicate or not), what the user allowed; at once when the plugins are listed again (Reload, install, remove); otherwise looked at again after a second, so a plugin built meanwhile is picked up. Turned off or on: not kept, counts at once |
| Profile: the clearance marks | both ends of every mark projected onto the centreline | the drawing (counted up when it is made again) and the marks themselves, compared (`profile::MarksAlong`) |
| Strings table | the structure as stored searched for once per string | nothing kept: once per structure (the rows themselves are kept by the Tables page) |
| OHL-Structure, Section library: the library's files | every file copied for the sheet each frame (again, and parsed again, with a form open); every file read and parsed again each second | shared, not copied; each file's document parsed once; the folder is listed and its files' times and lengths asked each second, and the files read again only when one is new, gone or differs — or was changed less than 3 s before it was read (its time may not tell a second change yet). A change made in the window is read at once |
| OHL-Structure: the section ids members choose from (four sheets) | the library's ids copied and the file's own looked up among them | the structure file's revision and the libraries' stamp (`SectionEditor::choices`) |

Left as they were, and why:

- **Phase distance, the project's copy of the values**: asked of the
  project on every frame (cheap now). It is not among the 15 library
  kinds whose change counts `revisions.libraries` up — another project
  line chosen does not count it for this kind — so that revision is no
  sure key for it.
- **The lists the plugins give** (table columns, check folders, scripts,
  class schemes, field presets) are still made on every call, from memory
  only.
- **`Model::unsolved_shown`**: with no section unsolved it only asks each
  section for its cases; left.
- **`Model::set_check_plugins`** compiles a check plugin's WebAssembly on
  the user-interface thread when the plugins in use change (a fraction of
  a second each): not part of this change.

## Edit benchmark (desktop application)

`bench::edit_benchmark` changes one section's stringing (as the span tool
and the section editor do) on a copy of a project and times the edit, one
weather case's update with and without a section to re-solve, the
jumpers' loads as an estimate and as their pull, the results' revision,
and compares the case brought up to date edit by edit with a full solve:

```bash
OHL_BENCH_FILE=/path/to/copy.ohl cargo test --release -p ohl-app edit_benchmark -- --ignored --nocapture
# OHL_BENCH_ALL=1: also with every case solved (minutes); OHL_BENCH_EDITS=N,
# OHL_BENCH_REPEAT=N: more edits or updates, to profile them
```

A real line (about 70 structures, 554 sections, 518 jumpers, 105
weather cases, jumpers' pull), Apple M-series, release, 2026-10-24:

| | before | after |
|---|---|---|
| one edit, the case shown solved | 1.9 s | 90 ms |
| one edit, every case solved | 139 s | 90 ms (the others solved again when wanted) |
| one case's update, nothing to re-solve, pull | 1.6 s | 70 ms |
| the same, estimated jumper loads | 0.4 s | 15 ms |
| the results' revision after an edit (a check page open) | 134 ms | 67 ms |

What it took, every result the same as before (the solver's tests and a
full solve compared): each jumper's solution kept per weather case with
everything it is solved from and taken back while that is unchanged
(`ohl_line::incremental::JumperMemo`, in the pull's passes too); the
routes made once per update instead of once per jumper (`Line::direction_at_in`,
the alignment points and the structures tied to them indexed:
`Line::alignment_views` was quadratic in the alignment points); the
jumpers' sagged lengths of the estimate kept by their exact inputs; the
taut-jumper test with the clamps indexed; the structures merged with their
types kept while a structure is unchanged (`Line::resolve_with`,
`ResolveCache`, two thirds of an edit); after an edit only the cases
shown, drawn per section, picked or asked for by a view in the last 2 s
brought up to date at once, the others not solved any more (solved
again from the line when wanted, as a case never shown is); an edit that
changes no solution (a report, the loads sent to the structures) not
solving anything; the line hashed as stored for the results' revision;
the 3D layers of the terrain, backgrounds and check results and the cut
panels' slabs no longer made again after an edit; the sheet preview over
the views redrawn once the edits settle (500 ms), the old one shown
meanwhile.

Then (2026-10-24, from the same review): the sheet templates the
Profile sheets window and the sheet preview ask for every frame kept for
a second and while the project's libraries are unchanged (they were read
and parsed from their files twice a frame); the profile's corridor (the
survey points, LiDAR points and 3D drawings projected onto the line)
kept while its centreline, width, ground, points and drawings are
unchanged (an edit made it again); the comparison of a project line shown
with the line worked on indexed by id; the thermal table's ratings
computed when what they come from changes, not every frame; the routes of
the structures, sections and jumpers worked out once per version of the
line (the Properties panel and the route worked on asked every frame);
the other wind side of the lines on the ground brought up to date from
its solution before (only the sections that differ), one solve at a time
per case, the solution before drawn meanwhile (a drag started a full
solve of the line per case and frame). A click picks in about 25 ms on
the real line (the 3 s once measured were before the fix of 2026-10-22).

A third round (2026-10-24): the jumpers' structures compared without
their stick drawing (views only; it was most of an update with the
jumpers' pull), the sections at each wire point indexed once per update,
the line checked (`Line::validate`) with its structures indexed by id,
the solved results walked once per case, the line hashed as JSON for the
results' revision (4.9 → 2.5 ms after an edit; 67 ms only the first
time, the library files), and the weather case tables of the Properties
panel laying out only the rows in view (two hundred with both wind
sides). A span edit on the real line stays about 79 ms (the solver's
update of the case shown, 53–57 ms with the jumpers' pull, 11 ms with
their estimate, is most of it).

The comparison with a full solve shows two sections on a real line whose
clamps differ by up to 0.17 m between the case brought up to date edit by
edit and a full solve (−20 °C, taut jumpers with their pull); the same
without the kept jumper solutions, so it comes from the pull's
iteration (different starting points), not from the speed work — kept as
it is for now (owner, 2026-10-24: unless a fix is fast), said in the
[jumpers' theory](../theory/jumpers.md#after-an-edit); on the roadmap (U11).

## Measuring benchmark (desktop application)

`cargo test --release -p ohl-app measuring_benchmark -- --ignored
--nocapture` (with `OHL_BENCH_FILE` for a project of your own, else the
synthetic line) moves the pointer across the 3D view for 120 frames, first
with Select held and the snap marker off (View › Show the snap point),
then with the marker on (it runs the click's search at every move), then
with **Distance** held after a first pick — every
frame then finds what is under the cursor (the tool click's own search)
and measures to it (owner, 2026-10-25: the measuring tools responsive
first).

| Project | Select: median (slowest) | Distance held: median (slowest) |
|---|---|---|
| synthetic line | 0.2 ms (0.3) | 0.2 ms (0.6) |
| a real line with its LiDAR (2 M points loaded) | 0.6 ms (18.5) | 0.6 ms (26.1) |

The search runs only when the pointer moved; the alignments' centre lines
for the stations are kept until the line changes.

**What can be clicked is kept** (owner, 2026-11-14: since the snap marker,
the 3D view of a real line lagged). The search made the list of every
clickable piece again at each pointer move — on a real line of 78
structures and 99 cases, 268 103 pieces in 16 ms, then all of them
projected: 30 ms a frame. Now the list is kept per model
(`selection::pick_set`: made again when the line's version, the case,
the snaps, the cases drawn or their solutions, the structure bodies or
the ground change), in runs of up to 32 segments of one object with
their box; a run whose box, projected, lies farther than the snap
distance from the pointer is left out (its segments project inside the
box's outline, so nothing nearer is missed). The pick is the same as
before, to the piece.

| A real line (78 structures, LiDAR) | Select, no marker | Select, marker | Distance held |
|---|---|---|---|
| before | 0.8 ms (10.9) | 30.0 ms (31.6) | 30.2 ms (45.4) |
| pieces kept, boxes | 0.8 ms (6.5) | 1.8 ms (31.2: the list made once) | 1.9 ms (2.5) |

**Kept measurements and notes**: `cargo test -p ohl-app
kept_annotations_benchmark -- --ignored --nocapture` keeps 300 annotations
on the synthetic line (100 clearances, 100 distances, 100 notes; three of
four another user's) and times computing them again, the views' overlay
and drawing them in 3D from 300 m and 2 km above the line's middle. Debug
build on the development machine, 2026-10-25, before → after: computed
again with others' measurements hidden 5.4 → 3.4 ms (shown: 5.6 ms, the
same); the overlay for the three views each frame 233 → about 2 µs (made
when the kept ones change, shared); drawn from 2 km 1 000 → 600 shapes
(no label beyond the label distance), from 300 m 1 000 → 668 shapes (the
overlapping labels and those off the view left out; 190 → 270 µs in the
debug build, the placement). A cursor standing still with Follow and snap
(frames asked for by something else): 108 → 1.6 µs a frame (debug,
synthetic span; the measurement found for that point taken again).

**Follow and snap**: `OHL_BENCH_FILE=… cargo test --release -p ohl-app
follow_benchmark -- --ignored --nocapture` follows a span in the middle of
the line (the default classes and radius): on a real line the span's
surroundings read in 23 ms, a cursor move 0.3 ms (the nearest point found
and measured), N (the closest spot of the span) 12 ms.

**Wire tools**: `wire_tools_benchmark` (same variables) times Clearance at
a point with its sweep along the span — 23 ms on a real span (on the
click; under the cursor only the point's own, without the sweep) — and
Wire to wire between two spans, 0.04 ms.

## Time budgets and exit clauses

Nothing may run forever (see the design goals). In code:

```rust
use std::time::Duration;
use ohl_core::budget::{Budget, CancelToken};

let budget = Budget::new()
    .max_iterations(100)                     // exit clause 1: iterations
    .time_limit(Duration::from_millis(50))   // exit clause 2: wall-clock time
    .cancel_token(CancelToken::new());       // exit clause 3: user cancel
let mut run = budget.start();
loop {
    run.step()?;           // returns Err(Stop) explaining why it stopped
    // … one iteration …
    # break;
}
# Ok::<(), ohl_core::budget::Stop>(())
```

Guidelines:

1. **Estimate** the expected duration of every potentially slow function and write it in its documentation.
2. Set the limit with a safety margin (≈10× the estimate) so that hitting it means a real problem.
3. When a limit is hit, **report why**: which limit, iterations done, elapsed time, and the solver's last state (e.g. residual). This is what lets us understand and fix the cause.
4. Never return an unconverged result as if it were valid.
5. Inner loops of microseconds (an element's Newton iteration, a 1-D search inside a solve) take an iteration limit only; the enclosing solve keeps the time limit and the cancel token. A run reads no clock without a time limit; with one it reads a shared counter kept from the clock once a millisecond (`ohl_core::budget::CLOCK_TICK`), so a time limit is noticed within about a millisecond, early or late (2026-10-26: reading the clock on every iteration cost a few per cent of a wire solve).

## Build profiles

- `dev` (debug builds and tests): the workspace's crates with line tables only (`debug = "line-tables-only"`: backtraces keep their files and lines, the build folder is several GB smaller and links are faster), the dependencies optimised (`opt-level = 2`) without debug information. For a debugger session with variables, build with `CARGO_PROFILE_DEV_DEBUG=true`.
- `release`: what is shipped (thin LTO, one codegen unit): the slowest to build.
- `bench-fast`: release optimisation without LTO, 16 codegen units — for local timing runs (`cargo test --profile bench-fast -p ohl-clearance --test zone_speed -- --ignored --nocapture`), much faster to build; its numbers are close to `release` but not the shipped binary's. Results do not depend on the profile.

## Magnetic field: performance

*Moved here from the theory page [magnetic-field.md](../theory/magnetic-field.md) on 2026-11-07: how the computation is made fast, with its measured times.*

What a computation costs and what was done to make it quicker without
changing a computed value (owner, 2026-10-29: "make sure to check the
performance"). Measured on 2026-10-29 on the owner's laptop (10 cores:
8 performance, 2 efficiency; release build).

**The cost.** Every point asks every straight piece of every wire:
the time is the points × the pieces × the time of one piece at one
point. The line the measurements were made on: 28 km, 7 circuits, 77
spans — **124 000 pieces** (the solver's, every subconductor's).

**One piece at one point, one core:**

| Arrangement | ns |
|---|---|
| piece by piece (`segment_field` summed) | 9.4 |
| per polyline (each vertex's distance once for the two pieces meeting there) | 4.7 |

Both give the same field within rounding (largest relative difference
\\( 3\cdot10^{-15} \\) on the test line, \\( 10^{-12} \\) required by the
tests).

**On all cores, sustained.** A laptop's cores slow down under full load:
a test line of 640 pieces went from 1 280 000 points/s in the first
second to about 600 000 points/s after 5 s, and stayed there. So a run of more than a few seconds goes at
about half the speed of a short measurement; the window's estimates use
the last run's speed, or a short measurement halved before the first run.

**What made it quicker** (each leaves every computed value as it was):

1. The sum per polyline (above): half the time per piece.
2. Points taken in **blocks of 64**: each wire's pieces are read once
   for the block and stay in the processor's cache, instead of the whole
   line's 7 MB of pieces being read again for every point.
3. The threads **take blocks in turn** instead of an equal share each,
   so the quicker cores take more of them.
4. A volume's **fine lattice only where a surface passes** (above):
   about half of it on a real span.

![Point by point, every point reads all the line's pieces; in blocks of 64 points each wire's pieces are read once for the block; the blocks wait in a queue and each thread takes the next](../images/theory/field-blocks.svg)

**Measured on the real line** (1000 A per circuit; one span of about
400 m, ±50 m, from the ground to 10 m over the highest wire; levels 1,
10 and 100 µT; times of the whole command, the line's solution included,
about 1.4 s):

| What | Points computed | Wires modelled | Time |
|---|---|---|---|
| 77 profiles of 101 points (every span) | 7 800 | whole line | 3.5 s |
| one profile, a cut at 1 m (after 1.–3.) | 9 600 | whole line | 2.2 s |
| one profile, a cut at 0.5 m (after 1.–3.) | 38 000 | whole line | 10 s |
| volume at 4 m (and a cut at 0.5 m), before 2. and 3. | 79 000 | whole line | 20 s |
| volume at 2 m (and a cut at 0.5 m), before 2. and 3. | 326 000 | whole line | 85 s |
| volume at 2 m, after | 326 000 | whole line | 67–70 s |
| volume at 2 m and a cut at 0.5 m | 326 000 + 38 000 | within 2 km (17 000 pieces) | 6 s |
| volume at 1 m | 1 740 000 of 4 400 000 | within 2 km | 63 s |

**What the wires modelled change** (`modelled_within`; the whole line's
field computed again at 200 points of the cut or volume):

| Within | Pieces (cut / volume) | Largest change (cut / volume) |
|---|---|---|
| 500 m | 4 900 / 7 900 | 0.55 % / 0.32 % |
| 1 km | 10 200 / 11 500 | 0.085 % / 0.080 % |
| 2 km | 15 700 / 17 200 | 0.024 % / 0.028 % |

**What the volume's step changes** (how far the surfaces lie off their
levels, at 200 points each: nine in ten within / at most):

| Step | 1 µT | 10 µT | 100 µT |
|---|---|---|---|
| 4 m | 0.16 % / 0.7 % | 2.5 % / 28 % | 71 % / 595 % |
| 2 m | 0.05 % / 0.2 % | 0.94 % / 27 % | 15 % / 55 % |
| 1 m | 0.01 % / 0.0 % | 0.27 % / 21 % | 3.7 % / 11 % |

The worst points lie next to the conductors, where the field bends
sharply between lattice points; the 100 µT surface (a tube of some 2 m
around each conductor) needs a step well under its radius.

**Not done, and why:**
- **Leaving out the far wires by default** — it changes computed values
  (a little: see above); it is the user's choice, with its effect
  measured at every run. The whole line stays the default (quality
  before speed).
- **Approximating far wires** (a multipole or a coarser wire far away) —
  it would change computed values without the user choosing it.
- **The GPU** — the field sums currents that cancel far from the line
  (a balanced three-phase line falls as \\( 1/d^2 \\)); single precision,
  all most GPUs offer, would lose digits there.

**Continuity between the views' tiles**. The values are
continuous: every point sums the same pieces of the whole line (or the
same within the distance asked). The drawings are laid on **one path
along a run of spans** (spans following each other along the line):
grids from above and volumes take each run whole, the stations along its
straights, the offsets across them, and at a line angle the offset along
the bisector, mitred (the offset point lies its offset from both
straights), the direction turning evenly along each straight from one
end's to the other's — so the lattice bends with the line, without the
gap on the outside of an angle and the overlap on its inside that boxes
per span left, and the isolines and surfaces run through the structures
unbroken (tested on a bent path: a tube around a bent wire closed but at
the path's ends). Where the offsets would fold over themselves (a very
sharp angle and a wide band) the run is refused with a narrower width
asked for. A level met exactly at a lattice point gives that point's
vertex to every edge through it (no crack; tested).

The structures' magnetic permeability is not modelled (above); the
jumpers and the structures' currents are.

## LiDAR clearance checks: how they are made fast

*Moved here from the theory page [lidar-analysis.md](../theory/lidar-analysis.md) on 2026-11-07: how the computation is made fast, with its measured times.*

LiDAR surveys hold \\( 10^7 \\) to \\( 10^8 \\) points per line, and every
vegetation point meets every position of every case. The work is kept to
the points and wires that can matter; the numbers computed are never
coarsened.

- **Only what can reach.** A point is compared only with the spans within
  its *reach*: \\( c_{\text{req}} + g + b \\) for grow-in, and the tallest
  tree looked for plus its root ball plus \\( A \\) for fall-in. Spans are
  indexed once per position on a plan grid (cells of about the reach), so
  a point meets the few spans near it, not the line.
  As built: each span is indexed once with all its positions (cells of a
  quarter of the reach, at least 10 m).
- **One bound per span before the exact distance.** Each span's positions
  (hot, left, right and the swing arc) fit in a capsule about its axis of
  radius \\( \max R + \) reach; a point outside it is rejected with one
  distance to a segment. Only points inside get the exact distances
  above, each a closed form (no iteration).
  As built: then a bound from **bands** — for every 20 m along the axis,
  how far from the axis and in which directions about it each position
  (and all of the span's together) lies; a position is worked out only if
  its bound can beat the best found. The nearest piece of a solved wire is
  looked for outward from the point's place along the axis, stopping when
  the places alone are further than the best. (The fall-in distance to a
  wire over the hinge circle is found by the Newton steps above, on an arc
  by the bound along the circle — not closed forms.)
- **Cases share the work.** Positions of all cases are gathered per span;
  the point is looked up once and compared with all its span's positions.
  The smallest margin found so far bounds the rest: a case cannot make a
  point worse once its distance exceeds the current worst by the reach.
  As built for fall-in: a tree is worked out fully (circle, resting angle,
  shelter) only when its bound is below 0 (it may count) or below the
  span's worst fall so far (it may be reported); a tree that becomes the
  span's nearest point or enters its worst list is worked out fully
  then. Counts never depend on it (a bound from 0 up never counts).
- **Ground once per point.** The height above the ground (for fall-in)
  comes from the terrain surface once per point; the fall sector's resting
  angle \\( \theta_{\min} \\) is computed only for points that are near
  enough, by stepping along the fall direction at the TIN's resolution.
- **Unbalanced ice only where trees are.** A span is solved with its own
  ice only if vegetation points lie within its reach (most spans over
  fields have none). Spans are independent: they are solved in parallel.
- **Isoclearance by refinement.** \\( c \\) is first computed on a coarse
  grid (e.g. 8 m). Because \\( c \\) cannot change faster than the distance
  moved, a coarse cell whose corner values all exceed the highest line
  drawn by more than its diagonal (times \\( \sqrt{1 + m^2} \\)) cannot hold
  a drawn line and is not refined. Only cells near the wires or near
  low clearance are refined to \\( \delta \\). As built: \\( m \\) is the
  steepest triangle near the cell; rows of cells in parallel.
- **Parallel by tiles.** Points are read tile by tile from the project's
  point cloud (its spatial chunks) and the tiles are processed in
  parallel; results are merged per span. As built: the points are read
  chunk by chunk into memory (13 bytes a point, whole millimetres), then
  checked in blocks of 65 536 points (about a stored chunk) on every
  core (the standard library's scoped threads, as the rest of the code);
  the results merged per span in a fixed order (the same answer whatever
  the threads).
- **Expected cost:** about 1 µs per vegetation point and position set
  today; with the swing arc and a few dozen cases, a line of \\( 10^8 \\)
  points (a third of them vegetation) is expected in minutes on a
  10-core computer, and an isoclearance drawing of a 50 km corridor in
  seconds. Exit clauses: a time limit per run, reported, never a partial
  result shown as complete.

  **Measured** (an invented 10 km line of 25 spans, three phases and an
  earth wire, a million invented vegetation points over ±60 m — a cleared
  strip of ±12 m and trees up to 28 m beyond, an unusually dense share of
  danger trees: 2–12 % of the points; a 10-core computer; the
  ignored test `danger_speed`): one case as before (k = 0) 0.4–0.7 µs a
  point on one thread, 0.08–0.18 µs on ten; one case with k = 0.1
  0.6–1.0 µs (0.1–0.2); one windy case with left, right and the swing arc,
  k = 0.1: 6–12 µs (0.9–2.1); twenty windy cases with their arcs (6000
  positions): 31–90 µs (4.6–14) — most of it the fall-in of danger trees
  against the arcs (the search along the hinge circle). Isoclearance of
  the 10 km corridor (±60 m, lines every 5 m to 40 m, 1 m grid refined
  from 8 m) for the twenty cases: 6–9 s (1.6 million clearances, most
  cells refined: lines run all along a corridor of that width) — a 50 km
  corridor about half a minute, not seconds; one case is about ten times
  faster. On a real line (930 spans of its sections, about 37 km of
corridor ±60 m, 18.7 million vegetation points of 100 million, 10
cores): one case as before, the check 1 s (0.05 µs a point); three cases
— hot, windy with left, right and the swing arcs, iced with unbalanced
ice on every span (930 solves) — with k = 0.1 and A = 2 m: the solves
13–23 s, the check 21–34 s (1.1–1.8 µs a point), the isoclearance
lines 13–14 s (9 million clearances). Exit clauses: `--time-limit` (default an hour) for a run in the
  command line, an hour in the application, checked between blocks and
  rows: an error, never a partial result.

## Clearances: how they are made fast

*Moved here from the theory page [clearances.md](../theory/clearances.md) on 2026-11-07: how the computation is made fast, with its measured times.*

Speed work coarsens nothing computed: every change below gives the
results of before **bit for bit** (`checks::reference`, the tests
`the_point_check_gives_the_results_of_before` and
`the_ground_check_gives_the_results_of_before`, on the example line in
five wind positions, with bundles, 30 000 invented points and ties on
whole metres; the earlier code is kept for them).

- **One measure.** A window checks one measure; the other is no longer
  worked out for every point and piece.
- **Bounds before the exact distance.** A wire piece is passed over
  without its exact distance when the distance to its bounding box (in
  plan within the zone's width, for the vertical) shows that it can give
  neither a smaller clearance than the smallest found nor one below the
  required clearance. Equal clearances keep the first found, as before.
- **The highest ground per cell** (`checks::GroundMax`, 5 m cells in
  tiles of 64 × 64): a piece (radially a sample every 0.5 m) whose lowest
  point is higher above the highest ground near it than the smallest
  clearance so far (and the required clearance) is passed over — for the
  radial, the ground within \\( r = \sqrt{(t + a)^2 + (s/2)^2} \\) of the
  sample (\\( t \\) the threshold, \\( a \\) the allowance, \\( s \\) the
  sample spacing): a triangle nearer in plan is under that ground, one
  farther is more than \\( r \\) away.
- **The reach for the ground, radially**. The radial
  clearance to the ground is kept within the reach \\( R \\) (the
  requirement plus `search_margin`) only, as for the points; a wire
  below the ground always counts. A piece or sample more than \\( R \\)
  above the highest ground within \\( R \\) of it in plan is passed
  over. Before, the smallest clearance so far was the threshold, so a
  wire tens of metres above a valley — every swing-arc line of its span,
  every 0.5 m — searched some 2000 triangles of a fine surface each, for
  a lowest outside the reach that was not exact either (the search
  stopped at \\( R \\) in plan).
- **The last sample's distance**. The distance to a
  surface changes by at most the distance moved, so a sample's lies
  within \\( d \pm m \\) of the last one searched (\\( d \\) its
  distance, \\( m \\) the distance between them): the upper bound
  narrows the search radius, the lower one passes over samples that
  cannot come below the threshold or within the reach — the same
  results.
- **Every core.** The ground check checks the wires in parallel; the
  points are read in blocks of 65 536, each block's stretches worked out
  on every core from the results so far and the results applied in the
  points' order (the same results as one point at a time). The solves
  were already spread over the cores, one wind position each.

Measured before and after on the invented long line (10 Pa steps, 131
solves, 10⁶ points; one run after the other on the same shared
computer):

| | before | after |
|---|---|---|
| solves | 68 s | 65 s (unchanged) |
| vertical, ground | 91 s | 12 s |
| vertical, points | 103 s (10 000 points a second) | 17 s (58 000 a second) |
| radial, ground | 264 s | 44 s |
| radial, points | 101 s | 15 s |
| the lowest clearances | 13.953, 0.000, 13.975, −0.015 m | the same |

A vertical check of the ground and of 10⁶ points with 10 Pa steps: 262 s
before, 94 s after, 8 s on the swing arc. With 10⁷ points the points'
time grows tenfold (about 20 minutes before, 4 after, 16 s on the arc,
extrapolated). With fine steps on a long line the **solves** are now
the larger part: one solve of every section per pressure and side,
spread over the cores — the swing arc is the remedy.

Ideas left out: *warm-starting each pressure's solve from the previous
one* — the solver stops at its tolerance, so a different start gives a
different (equally valid) position: not the same numbers. *Solving only
the sections near candidate points* — a ground rule has the ground under
every span, and which LiDAR points are near a wire is known only after
the solve; a bound from the still and full positions would rest on the
rigid-swing simplification. *Ignoring the depth and comparing each point
with the line's points only* (the owner's suggestion for the vertical) —
the vertical clearance is the wire's height over the point's plan
position: taking the nearest polyline point instead of the piece over it
would change the numbers by up to the rise of a piece over half its
length (centimetres on a slope), and the depth is a setting of the rule;
the speed comes instead from the plan box test, which looks at exactly
the pieces within the width.
