# Desktop application

`ols-cad` opens a line file or a project and shows it in dockable views that share the selection and the weather case.

```bash
cargo run --release -p ols-app -- examples/synthetic-line/synthetic-line.ols.json \
    --terrain examples/synthetic-line/synthetic-survey.csv \
    --weather examples/synthetic-line/weather-cases.olswc

cargo run --release -p ols-app -- project.ols          # with the project's point cloud
```

| Option | Meaning |
|---|---|
| `--library` | Wire library (file or directory); default: the `*.olsw` files next to the model, else `libraries/cables`. |
| `--structures` | Pylon files; default: the `Structures/` folder next to the model. |
| `--weather` | Weather cases (`.olswc`); default: still air at 15 °C. All cases are solved. |
| `--terrain` | Survey points for the ground. |
| `--pointcloud` | Point cloud in a project (`FILE.ols` or `FILE.ols#name`); default: the project's own. |
| `--points` | Most LiDAR points shown (default 2 000 000). The ground surface is built from all ground points (lowest per 1 m cell) in the corridor. |
| `--corridor` | Half width of the LiDAR corridor around the spans (default 60 m). |
| `--screenshot FILE.png` (`--focus ID`) | Load, render the 3D view to an image and exit. |

## Views

- **3D** — ground (shaded), LiDAR points coloured by class role (vegetation green, buildings red, water blue, wires yellow…), structures, insulator strings, wires in their phase colours, alignments. Drag to turn, right-drag or shift-drag to pan, scroll to zoom, double-click to fly to an object. Height exaggeration in the Model panel.
- **Plan** — alignments, wires and structures from above, north up, with a scale bar. Drag, scroll at the pointer, double-click to centre.
- **Profile** — per route: ground along the centreline, structures and wires by station; heights fit the visible stretch. Scroll zooms along the route, drag moves, double-click shows the whole route.
- **Properties** — the selected structure (position, type, attachments, spans through it) or span (cable, circuit, electrical system and voltage, bundle, horizontal and maximum tension, sag and lowest point in every weather case). Links select related objects.
- **Model** — layers, weather cases, what was loaded and how long it took.

Clicking an object in any view selects it everywhere. Tabs can be dragged to rearrange the window.

Loading runs in the background. On a real 25 km line with 99.6 million LiDAR points stored in its project, the window is ready in about 8 s (release build): 84 million points decoded in the corridor, 1.5 million shown, a 5-million-triangle ground surface.
