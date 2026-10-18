# Performance and time budgets

## Benchmarks

Performance-sensitive code has [criterion](https://github.com/bheisler/criterion.rs) benchmarks in `crates/<crate>/benches/`:

```bash
cargo bench --workspace --bench '*'
```

The CI workflow is started by hand (GitHub Actions › CI › Run workflow); with **full** ticked it also runs the benchmarks: it benchmarks the parent commit on the same machine first and fails if a benchmark becomes more than 1.5× slower. Shared CI machines are noisy, so the threshold is deliberately loose; local runs give precise numbers.

Example: `units/total_load_typed_10k` vs `units/total_load_raw_10k` check that typed quantities cost nothing compared with plain `f64`.

## Frame benchmark (desktop application)

The desktop window is drawn headless (egui_kittest with wgpu, as the GPU tests) with the 3D view at four distances (the whole line, 3 km, 600 m and 80 m from the middle structure) and the plan view at four zooms (the whole line, 3 km, 400 m and 60 m wide). For each view it settles (every chunk built, every point cloud node read), checks that an idle frame asks for no other one, then draws 20 frames while turning or panning a little (as a drag does) and reports the median and slowest frame, the time to tessellate the frame's shapes, the shapes, what the 3D view drew, the time of a click (picking) and the time of each phase (`crate::perf` spans: frame hooks, menus, each tab, 3D layers rebuilt, chunk plan and build, point stream, render, labels; plan wires and labels; …):

```bash
OLS_REQUIRE_GPU_TESTS=1 cargo test --release -p ols-app frame_benchmark -- --ignored --nocapture
# a project of your own (work on a copy: it is opened for editing), with
# your view preferences (a ViewPrefs JSON) and frames per view:
OLS_BENCH_FILE=copy.ols OLS_BENCH_PREFS=prefs.json OLS_BENCH_FRAMES=40 cargo test --release …
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

### What is drawn at which distance

Display only: calculations, checks and picking always use the full model; nothing here changes a computed number (`ols validate benchmarks` and the CLI's wire, span, load and usage outputs are identical before and after these changes). The distances of each layer's levels are the Level of detail table's (per user; defaults in `lod::LodRow::default_for`), what each level keeps is fixed (`scene::structures`, `scene::wires`, constants `lod::DASH_SCALE` and `lod::BODY_DOT_PX`):

| Layer | Level 0 (default to) | Level 1 (to) | Level 2 (to the render distance) |
|---|---|---|---|
| Terrain, other surfaces | every TIN triangle (1.5 km) | 8 m grid (5 km) | 32 m grid (30 km) |
| LiDAR | streamed by node under the point budget (5 million) and the error on the screen (1.5 px), to 3 km | | |
| Structures | lattice members, arms, strings (2 km) | mast line, arms, strings (8 km) | mast line (50 km) |
| Wires, other cases, project lines | subconductors and yokes, clamps and bodies to their shape, bare points' marks, jumpers' support clamps, line codes in 1.5 m units (1.5 km) | one line per bundle, 16 points per span, dashes × 4, bodies as 4 px dots, spreads, spacers, arrows (6 km) | 5 points per span, dashes × 16 (50 km) |
| Sections not solved | dashed red, 1.5 m units | dashes × 4 | dashes × 16 |
| Drawings (DXF) | every vertex (1 km) | simplified to 0.5 m (4 km) | 2 m (10 km) |
| 3D labels | sags, section labels, points', strings', wire points' labels within the label distance (1.5 km) | | structures' names at any distance, placed first |

Plan: what is in view, half-pixel thinning, structures' names at every zoom (overlapping ones left out, the first in line order and the selected one kept). Profile: one survey dot per pixel, half-pixel thinning, shapes kept while the view stands still.

### Repaints

An idle window asks for no frame. Only these ask for frames while they run: a camera key held (every frame), chunks or point cloud nodes still to build or read (every frame, 50 ms), a structure file open in the structure application (its file looked at twice a second), solving in the background, jobs and servers (their own intervals).

## Time budgets and exit clauses

Nothing may run forever (see the design goals). In code:

```rust
use std::time::Duration;
use ols_core::budget::{Budget, CancelToken};

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
# Ok::<(), ols_core::budget::Stop>(())
```

Guidelines:

1. **Estimate** the expected duration of every potentially slow function and write it in its documentation.
2. Set the limit with a safety margin (≈10× the estimate) so that hitting it means a real problem.
3. When a limit is hit, **report why**: which limit, iterations done, elapsed time, and the solver's last state (e.g. residual). This is what lets us understand and fix the cause.
4. Never return an unconverged result as if it were valid.
