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
| `--controls` | Navigation scheme: `orbit` or `minecraft` (default: the one used last). |
| `--screenshot FILE.png` (`--focus ID`, `--distance M`) | Load, render the 3D view to an image and exit (with `--focus`, looking at that structure's first attachment). |

## Views

- **3D** — ground (shaded), LiDAR points coloured by class role (vegetation green, buildings red, water blue, wires yellow…), structures, insulator strings, wires in their phase colours (bundles as their subconductors, `bundle_spacing` apart — one section, solved and selected as one), alignments. Navigation: see [Controls](#controls). Height exaggeration in the Model panel.
- **Plan** — alignments, wires and structures from above, north up, with a scale bar. Drag, scroll at the pointer, double-click to centre.
- **Profile** — per route: ground along the centreline, structures and wires by station; heights fit the visible stretch. Scroll zooms along the route, drag moves, double-click shows the whole route.
- **Properties** — the selected structure (position, type, attachments, spans through it) or span (cable, circuit, electrical system and voltage, bundle, horizontal and maximum tension, sag and lowest point in every weather case). Links select related objects.
- **Model** — layers, weather cases, what was loaded and how long it took.
- **Snap** — what can be clicked.
- **Properties** of the other kinds: an attachment point (string type, insulator and wire points, length, weight, spans and jumpers there), an alignment point (position, tied structure), a LiDAR point (position, class and roles).

## Controls

The 3D view is navigated with the mouse and, while the pointer is over it, the keyboard — like Minecraft in creative mode:

| Input | Action |
|---|---|
| W / S | forward / back along the ground, in the direction of view |
| A / D | left / right |
| Space / Shift | up / down |
| Ctrl (held) | four times faster |
| scroll | zoom |
| right or middle drag | pan |
| double-click | fly to the object |

Two schemes, chosen in the Model panel (Controls):

- **Orbit** — left drag turns the view around the point looked at.
- **Minecraft creative** — first person, as in the game: clicking the 3D view captures the mouse (locked and hidden); moving it looks around (mouse down: look down); a crosshair marks the centre and a click selects what is under it (double-click flies to it); **Ctrl + wheel** sets the speed (the wheel alone does nothing). **Escape** frees the mouse to use the panels (switching to another application frees it too).

The speed (m/s) and, for the first-person scheme, the look sensitivity are set there too, and the list shows every key of the scheme. The scheme, speed and sensitivity are remembered for the next start; `--controls orbit` or `--controls minecraft` chooses one at start. Schemes are data (`controls.rs`: actions bound to keys and mouse buttons), so schemes familiar from other software can be added as presets, and later edited and saved.

The **Snap** tab (next to Model) chooses what can be clicked: structures, spans, jumpers, attachment points, alignment points, LiDAR points (more kinds will be added); by default structures, spans and jumpers. At about the same distance, a point wins over the line it sits on and a structure over a wire. The choice is remembered.

Clicking an object in any view selects it everywhere: a structure anywhere along the lines it is drawn with (mast or stick drawing, arms, insulator strings), a span anywhere along its wire, a jumper anywhere along it (drawn as a bundle like its section). The properties of a jumper show its cable, section, bundle (and whether it is its own or its section's), length or sag, support string, and results in every weather case. Tabs can be dragged to rearrange the window.

Loading runs in the background. On a real 25 km line with 99.6 million LiDAR points stored in its project, the window is ready in about 8 s (release build): 84 million points decoded in the corridor, 1.5 million shown, a 5-million-triangle ground surface.
