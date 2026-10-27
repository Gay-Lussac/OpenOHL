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
Export drawings window and the sheet preview ask for every frame kept for
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
with Select held, then with **Distance** held after a first pick — every
frame then finds what is under the cursor (the tool click's own search)
and measures to it (owner, 2026-10-25: the measuring tools responsive
first).

| Project | Select: median (slowest) | Distance held: median (slowest) |
|---|---|---|
| synthetic line | 0.2 ms (0.3) | 0.2 ms (0.6) |
| a real line with its LiDAR (2 M points loaded) | 0.6 ms (18.5) | 0.6 ms (26.1) |

The search runs only when the pointer moved; the alignments' centre lines
for the stations are kept until the line changes.

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
