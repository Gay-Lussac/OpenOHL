# The structure application

`ols-structure` edits one structure file (`.olss`) in a window of its
own, from level 1 (a stick figure: the points where insulators hang) to
level 3 (a full design: every member with its section, the supports).
See [Building structures](../development/structure-building.md) for
what the levels mean and what comes next.

```bash
ols-structure examples/synthetic-line/Structures/type-02.olss
```

Options: `--insulators DIR` (the insulator assemblies strings may take;
default: the file's folder and an `Insulators` folder there) and
`--ahead DEG` (which plan angle means ahead along the line, for the
compass). `--project`, `--line` and `--structure` are reserved for
opening a placed structure of a project directly; they are not built yet
and say so.

From the line application, **Edit…** on a structure (in its properties,
or on the Structure types page) starts it on the structure's file; saved
here, the line follows ([desktop application](desktop-app.md#the-pylon-workspace)).

## The window

- **Header**: **Level N — name** (worked out from the file, never
  stated), the structure's id and name, its file, *\* unsaved*. Under it,
  **for level N + 1:** what the next level still lacks — no member yet,
  a member without a section (one item per member), no `[[section]]`, no
  supported joint. Each item is a link: it opens the tab and table where
  it is done and picks its row. **advice (n)** (folded) lists what reads
  but is worth a look, never counted in the level: a point on no joint
  or member (it carries no load in an analysis), a joint on one member
  or none, two members crossing without a joint (X-bracing bolted at the
  crossing is a common case).
- **3D view** on top, in the structure's own axes: x to the right of the
  line (red), y ahead (green), z up (blue), 0;0 at the centre, the ground
  grid at the level of the feet and a compass. A click picks a joint,
  point, wire point or member, Shift + click picks another; a drag from a
  joint or point moves it (snapped to the grid set, or onto a joint) — one
  undo step per drag; elsewhere a drag turns the view, a right-drag pans,
  the wheel zooms. **Fit** shows the whole structure.
- **Tabs** underneath:

| Tab | Tables | Tools |
|---|---|---|
| **Geometry** (level 1) | Attachment points, Insulators (top, wire point, kind, assembly, length, angle, direction, weight, wind area, compression, description), Wire points (counterweight), Set points, Structure (name, height, feet, height extensions) | **+ Point**, **I** and **V** presets for the points picked, **Delete…** (lists what goes with it first) |
| **Body** (level 2) | Joints (x, y, z), Members (ends, length) | **Add joint** (above the joint picked), **Member** between two joints picked, **Point here** on a joint, **Delete…** |
| **Design** (level 3) | Supports (**Support the feet** pins the lowest joints), Members (section, kind, buckling factor k), Sections and Materials (their fields typed with units, e.g. `1500 mm2`) | — |
| **Load cases** | to come: the load-case recipes of the structure and the load table per placed structure ([Structure load cases](structure-load-cases.md)) | — |
| **Results** | to come: the FE and pole checks per load case, deformed shape, usage colours, report | — |
| **In the line** | to come: the places using the structure and their loads | — |

## Undo, saving, closing

Every change is one edit of the file, undone with **Ctrl+Z** (⌘Z) or
**⟲ Undo** and made again with **Ctrl+Shift+Z** or **⟳ Redo**; the Edit
menu names the edit. A value dragged in a table, or a point dragged in
the view, is one step. Comments and the order of the file are kept.

**Save** (Ctrl+S) writes the file; **Save as…** (Ctrl+Shift+S) another
one, which is then the file edited. A file that does not read (the error
is shown in red) is not saved. Closing the window with unsaved changes
asks first: **Save and close**, **Close without saving** or **Cancel**.
