# Join routes from another project

*Built 2026-11-25, in active development, not approved yet*
([decisions](../development/decisions.md#2026-11-25-one-model-per-line-the-join-routes-command-built),
roadmap O17, *One model per line*).

**A line and its branches are one model**: a line branching at a Y or T
tower, two or more routes sharing a tower, is one project, its shared
towers one object each ([Towers shared by several routes](shared-towers.md)).
Only separate lines — meeting at substations, or where another line's
circuits ride on the same towers — are separate models. The split is
decided when the network is designed; where a line was nevertheless
kept in several project files (a branch drawn apart, for example),
**File › Join routes from another project…** joins the other file's
routes into the open project.

`ohl network import` is unchanged (one project file = one line), and so
are twins between separate lines (listed by the import for an admin,
never merged, taken as one tower by the network's checks).

## The Join routes window

![The Join routes from another project window: the sentence and User guide button; 1 The project file to join, branch.ohl, joined into the project line main; 2 Ids renamed, the prefix branch; 3 Twins, the twin distance 0.5 m; the preview: from branch.ohl, routes branch, the objects counted, the ids renamed (structure T3 to branch-T3, alignment point P2 to branch-P2, system ex220 to branch-ex220), the library entries, and the twin T3 with T3 of the other file, ticked, 0.000 m apart, height, level and turned all zero, one shared tower; then the buttons Join and Cancel](../images/user-guide/join-routes.png)

**What it is for**: to make one model of a line kept in several project
files — the other file's routes, structures, sections and everything they
refer to brought into the open project, its towers that are the same as
yours offered to become one shared tower each.

**How to use it**:

1. Open the project that is to hold the whole line (File › Open), on the
   project line the routes are to join (the main line, or a project line
   of your own to look at the result first).
2. **File › Join routes from another project…**.
3. **1** — **Choose the file…**: the other project file (`.ohl`). It is
   read, never changed. The window then shows what will be joined.
4. **2** — check the **Prefix** given to the other file's ids that
   your project has already.
5. **3** — check the **Twin distance**, then look at the **Twins**: tick
   **make one shared tower** for each tower that is the same as yours
   (none is ticked to start with). The state of each ticked twin is shown
   at once: *one shared tower*, or *refused* with the reason.
6. Settle every library **conflict** (tick it, or make the entries the
   same in one of the projects first).
7. Read **To look at**, then **Join**. Escape or **Cancel**: nothing
   changes.

**The window**:

| Field | What it does |
|---|---|
| 1 The project file to join | The other project file; its **main project line** as it is now (uncommitted changes too) is joined. The open project itself is refused. |
| Joined into the project line | The project line you work on: the join goes there. |
| 2 Prefix | An id of the other file that your project has already — a route, alignment, alignment point, structure, section, jumper, spacer, electrical system, circuit (by id or name) or structure group — is renamed `PREFIX-id` (`PREFIX-id-2` when that is taken too), with everything that refers to it, as the network import renames ids. 1 to 32 letters, digits, `_`, `.` or `-`; default: from the file's name. |
| 3 Twin distance (m) | Towers of the other file of the **same type** as one of yours, at most this far from it in plan, are listed as twins — **0.5 m** by default, the network import's twin search. |
| What will be joined | The other file, its line, its revision (or *uncommitted*), its routes (as they will be named) and the objects counted. |
| Ids renamed | Each id renamed, from → to (a box that scrolls). |
| Library entries | Each entry of the other file's library files, mapped as the network import maps them: **the same** as yours (reused), **added** (to your file of that name; a file holding one entry whose name you have already: under a free name, `type-2.ohls`), **differs** (a conflict, with its tick), or **left in the file** (a draft's file, a file that is not TOML). |
| use this project's entry for the joined routes | Under a conflict: ticked, your entry is used by the joined routes too (their results may change: said under *To look at*). Not ticked (the default): **Join is refused** — nothing is guessed. |
| Twins | Each twin: **make one shared tower** (the tick, off by default), your tower, the other file's (and its id if joined apart), how far apart, the **height** difference (the highest point above the tower's ground point), the **level** difference of the ground points, how much it is **turned**, and what becomes of it (hover for the details). |
| To look at | What changes for the joined routes or your towers: a joined structure whose load cases change by your structure groups and their rules, a line setting of the other file that differs (yours apply to the joined routes), a library entry accepted or left. |
| Done too / Left in the other file / Refused | What else the join does (a shared tower's routes given on it, own lists joined); what stays in the other file (its history, other project lines, point clouds, backgrounds, surfaces, users, notes); why Join is not possible yet. |

**Buttons and keys**: **Join** (greyed while something is refused; its
hover says what), **Cancel**. **Escape** cancels; **Enter** does not
join. Long lists scroll in boxes of fixed height; the window closes with
its ×.

### A twin made one shared tower

- **Your tower is kept** as it is: its id, position, orientation, type,
  height, strings and wire points — nothing of it is changed silently.
- The other line's **sections, jumpers and circuit starts** on its tower
  are re-attached to your tower **by wire point label**: the wire point
  they end on or pass through (by its set) has a label; your tower's wire
  point of that label, and its set, takes them. Its **jumper clamps**
  keep their wire point's label; its **alignment points** tied to its
  tower are tied to yours (moved under it); its structure groups name
  yours.
- **A label your tower has not** (or has without a set) **refuses that
  twin**: it stays two towers, and the window lists why — nothing is
  guessed. Change the labels in one of the projects (OHL-Structure, or
  the structure's own wire points) and choose the file again.
- The merged tower is then **shared by several routes**: the rules of
  [Towers shared by several routes](shared-towers.md) apply to it. Where
  the alignments alone would not put it on both lines' routes, its routes
  are given on it (said under *Done too*).
- The twin's height, level and turn are listed so you can see whether it
  really is the same tower; **NOT ON THE SAFE SIDE** if ticked when it is
  not: the other line's wires would then hang on your tower's wire
  points, which may be higher, lower or turned — the sags, clearances and
  loads of the joined sections change. What makes it safe: tick only a
  twin you know is one tower (the same structure drawn in both files),
  look at its height, level and turn first, and run the checks after the
  join.

### Good to know

- **One change, one revision**: the library files written and the
  merged line are **one change** — **Edit › Undo** takes all of it back,
  exactly — and the revision *joined routes from FILE* is made on the
  project line you work on (Versions › History). Undo after it takes the
  line back; the revision stays in the history, as any revision does.
- **Refused**: a file holding a part of a line (a working set: join the
  whole line); a file in another coordinate system (reproject one of
  them first, Terrain › Coordinate system); a library conflict not
  ticked; a merged line that is not valid (the reason is said). With a
  working set of your own, the joined routes are outside it: the change
  is refused as any change of another route is.
- **Your settings apply**: the joined routes take your project's weather
  cases, criteria, calculation options and electrical settings; each one
  that differs in the other file is listed under *To look at*.
- **Load cases**: a joined structure in no structure group takes every
  case of your load case library; your groups' rules may put it in a
  group (fewer cases). Each such change is listed under *To look at* —
  check it before you join.
- **On the command line**: `ohl project join FILE --into PROJECT --user
  NAME [--prefix P] [--merge OPEN=OTHER]… [--accept KIND/FILE/ENTRY]…
  [--dry-run]` prints the same plan and writes the same change and
  revision ([the command](project.md#join-routes-from-another-project-on-the-command-line)).
