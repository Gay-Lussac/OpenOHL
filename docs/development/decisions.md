# Decision log

Every decision of the owner, and every change to how a calculation or a
format works, is recorded here with its date, newest first. The theory
pages, the user guide and the [data model](data-model.md) describe the
result; this page says *what was decided, when and why*. Nothing here is
approved as finished: everything is built and in active development
(not approved yet). Open questions are in the roadmap (the open points
of Parts A, B and C).

## 2026-10-27

**Clearances tab: a usability pass (owner, 2026-10-27)**

The UI review of 2026-10-26 had skipped "Clearances tab rework". Looked
at as a line engineer's page; nothing removed, results and formats
unchanged:

- **Results before the editor**: the violations came after the rule
  editor (long when opened), so a run's answer was below the fold. Now:
  the rule file, the results, then Edit rules, then the ground windows.
- **The rule file named**: the dropdown had no label; now "Rule file",
  its number of rules beside it, and **Add…** for another file (before,
  only offered while there was none).
- **No box in a box**: the violations had their own scroll area inside
  the tab's (cut off, two scroll bars); they now scroll with the tab,
  like the other check windows' bodies above their Run footer.
- **One violation, two lines**: the wire (and the other one) strong on
  the first; clearance found < required, rule and class on the second
  (the single long line was cut off in the narrow tab).
- **Before the first run** the tab says "Not checked yet" and what Run
  does (it showed nothing).
- **Vertical… / Radial…** (the ground and LiDAR clearance windows) are
  reachable from the tab also when a rule file is kept.
- Clicking a violation selects only its wire (clearing a multiple
  selection) as the other "go to" links do.
- Not done: screenshots for the docs (none taken in this pass).

**Several objects selected: Shift and Cmd/Ctrl+click (owner, 2026-10-27)**

- The selection held one object. Now Shift+click adds, Cmd+click
  (Ctrl+click on Windows and Linux) adds or takes out, in the 3D, plan
  and profile views and with a table row's ⤴. The one clicked last stays
  the selection (`Ready::selection`: Properties, the tools, Go to…); the
  others are `Model::selected_also` (with the model, as
  `selection_case`, so every view highlights them without new
  parameters: `selection::all_selected` / `is_selected`). A view reports
  its click (`selection::clicked`, egui's temporary data); the page
  adds or toggles it (`selection::multi_click`) when Shift or Cmd/Ctrl
  was held, else the one clicked is the only one. Highlighted in every
  view on every drawn copy (the 3D selection layer draws each), the plan,
  the profile and the tables.
- **Delete selected** with several: one command (a batch of the
  deletions, sections and points before the structures; a section once
  for several of its spans), confirmed as before; the confirmation
  shows what the first deletion needing one takes (the others' are
  taken too; one undo brings all back). A span point or spread with
  spreads is refused in a multiple deletion (it asks which, on its own).
  For this the session deletes an object once when several deletions of
  one command take it (`ohl_session::deleted_once`: two structures and
  the jumper between them); a single deletion is unchanged.
- **Escape** clears the selection (all of it) when no window, tool,
  chain or drag takes the key and the 3D view does not hold the mouse.
  Selected objects that an edit removed are no longer selected.
- Not in the cross-section views yet (they highlight the selection
  only).

**The Tables page keeps its rows (owner, 2026-10-27)**

- A table's rows were built every frame (each structure's sections
  counted, routes and circuits found, the load types read from the
  project file). Now they are kept and built again only when what they
  are built from changed (`TablePage::cached_rows`, key `RowsKey`: the
  table and weather case, the line's version, the library files'
  revision, the ground, each case's solution, the display cases, the
  model's files, the plugin columns, editable). For that a solution has
  a serial (`Solved::serial`, new whenever a case is made or solved
  again), and saving the project's load types counts the library
  revision up. Display only: the same rows; filter, sort and Apply
  unchanged. Measured in performance.md (Tables page).

**The server encodes each broadcast once (owner, 2026-10-27)**

- Left open on 2026-10-26 (see "Not done" there): every broadcast
  (edits, locks, the users' list) was serialised to JSON once per client,
  in each client's writer, and the message itself copied per client by
  the broadcast channel. Now `server::Event::new` encodes it once
  (`Utf8Bytes`, a shared buffer) and shares the message (`Arc`): the QUIC
  writer writes those bytes (`write_encoded`, the same length prefix and
  64 MiB limit), the web client's writer sends them as its text frame.
  Answers and notices of one client are still encoded by its writer.
- Behaviour identical: the same JSON, the same order (the answer right
  after its edit), the same limits; tests that every receiver holds one
  buffer and that three users receive the same edit.

**Library files the export lacks are added on project import (owner, 2026-10-27)**

- An export cut off before its cable data imported with no cables and
  every section unsolved. File › New project from another program… and
  Import › Another project into this one… now have **Add library
  files…** (files picked; their kind from the extension,
  `ohl_io::projects::LIBRARY_EXTENSIONS`: the project's library kinds
  but the structure loads); the command line `--library FILE`
  (repeatable) on `ohl project import-from` and `import-into`.
- They are added to what the plugin read before anything else
  (`ImportedProject::add_libraries`): before *clip in*, which needs the
  wire types; a file of the same kind and name as one read replaces it;
  each is noted. Into an existing project they are therefore merged by
  the rules of the import's own files (cables by id into the project's
  wire file). A file of no library kind is refused before reading.
- No format changes.

**Replace mode: sections crossing the stretch's end are split; circuits the variant lacks are removed (owner, 2026-10-27)**

- A project section running through a tower the variant removes and on
  beyond the stretch's end was removed whole, the part outside too. Now
  it is split at the end: its spans with an end outside the stretch stay
  as a section of their own (the first piece under its id; span points,
  local winds and stringing of those spans kept — a clipped-in section's
  lengths, a sag in a span left out taken in the ruling span;
  `Section::piece`), the part inside goes.
- Inside the stretch, the sections of a circuit the variant does not carry
  at all (e.g. one the variant re-equips a tower without) stay removed by
  default, as for the others; the list offers to keep them.

**Imports get everything in, the rules are checked afterwards; curves held flat (owner, 2026-10-27)**

- The owner: "what's important is that the imports get everything in; the
  rules are to be checked afterwards." A wire file is no longer refused
  for one faulty wire type: the wire library leaves that wire type out
  with its reason (`Library::skipped`) and loads the rest; the import
  keeps the file as the export gave it (safe mode no longer strips
  cables), and the reasons are shown when the library loads (app load
  log and library page, `ohl line` warnings).
- A stress-strain curve that stops rising before 1 % strain, without a
  range given (every PLS-CADD export; an OPGW's aluminium layer turning
  at 0.63 %), was refused. The owner chose to accept it and hold the
  curve flat from where it stops rising: the layer takes no more load
  beyond, the wire stretches more (conservative for clearances, marked as
  such in the theory page), said when the library loads and warned about
  in every state beyond it. With a range given, a curve that turns inside
  it is still left out (the data contradict themselves).
- Another project into this one: a merged line that does not check (a
  variant labelling the phases or systems of a conductor the project
  continues otherwise — a by-pass reconnecting circuits) was refused at
  Import. It is now mended as safe mode mends a plugin's line (the later
  section of a joined pair takes the first's system and phase, same
  voltage; else the joining jumper is left out), each mend said in the
  notes, to be checked afterwards. **In Replace mode the variant's names
  win** (owner, 2026-10-27: the variant is the new state): a project
  section joined to an imported one takes the imported one's system and
  phase, and the names run along the whole conductor (the project's
  sections outside the stretch too); in Add mode the project's win.
- Versions › Project lines › Working tree: a project line with nothing
  of its own committed and uncommitted work was drawn joined to its
  source twice (its start ring and its open dot); the open dot now follows
  its start ring (owner report).

**Another project into this one: Replace mode for a variant, a window that fits (owner, 2026-10-27)**

- **The window was too big**: with many objects in both, Import went off
  the screen and the window did not scroll. Now what is to settle
  scrolls (its height kept to the screen), and the summary line, Import
  and the status stay below it.
- **Replace mode, the default** (`ohl_io::import_into::ImportMode`;
  `ohl project import-into --mode replace|add`): "usually each thing that
  is the same in both should be replaced by the new one", and an import
  that covers part of the line is a variant of that stretch — the line
  outside it kept, inside it the import's towers, sections and jumpers
  instead of the project's. What appears in both defaults to the
  imported one (cables too: the imported cable replaces the project's in
  its wire file, under its id; *keep both* stays a choice, never the
  default). A tower in both is the imported tower (choice per tower,
  key `tower:<id>`); the project's sets it lacks stay on it where what
  the line keeps hangs on them. A tower of another id at the same place
  takes the imported id, what refers to it following.
- **The stretch**: two towers in both the import goes between with no
  other tower in both between them (along its sections, over its own new
  towers too; along its alignments when it has no section) mark a gap;
  the project's structures in that gap along its alignments or sections
  are removed, with the sections at them and those over towers in both
  only whose spans the import covers, and the jumpers at them or with
  every end inside the stretch. Jumpers and sections at the stretch's end
  towers that go on outside are kept. A section that runs out of the
  stretch through a removed structure goes with it (a note says so).
  Every removal is listed with why and a choice to keep it (key
  `remove:structure|section|jumper:<id>`). An imported alignment on one
  of the project's replaces the part between their first and last points
  in common.
- **Add mode** is the behaviour before: the project's kept, nothing
  removed.
- Checked on a real export pair (a variant of a stretch of a line, kept
  out of the repository): the stretch's three structures not in the
  variant and their sections and jumpers removed, the tower at the same
  place under the variant's id, the new tower added, the line checks;
  the sections that do not solve are only those on two cables whose ids
  the wire library refuses (spaces in the id, from the export).

**Safe mode for projects from other programs; solid wires filling their circle (owner, 2026-10-27)**

- Exports with faults (an id given twice, empty values, joined sections
  given two systems, a wire file with a cable twice or one that does not
  load) refused the whole import. The owner asked for a safe mode that
  imports what is there with a warning that some things may be missing.
  The host now reads a project plugin's line and wire files as far as they
  read (`ohl_io::projects::line_safe`, `safe_cables`), each fault said as a
  `warning:` in the notes; nothing is refused for one fault. Two joined
  sections of different systems of the same voltage take the first's
  system and phase rather than losing the jumpers between them (an export
  naming one conductor two ways); of different voltages, the jumper is
  left out.
- A cable's area was refused when it exceeded its diameter circle by any
  amount; a solid 50 mm² wire of 8.00 mm (50.3 mm² both, rounded) failed.
  The check guards against unit mistakes (factors of 10 or more): it now
  allows 1 % over the circle for rounded data.

**LiDAR class schemes never block the points; plugins may ship them (owner, 2026-10-27)**

- Since the old names were dropped (2026-10-26), a class scheme file still
  in the old format stopped the LiDAR import (app and `ohl pointcloud
  import --classes`), and a point cloud that had stored one stopped the
  whole project from opening. Now a scheme that cannot be read is not
  used: the import goes on with the standard classes (ASPRS) and says so;
  a cloud whose stored scheme no longer reads opens with the standard
  classes, said in the load log. The points keep their class numbers;
  a scheme chosen later (Class colours) names and colours them.
- Plugins may ship class schemes as files (`[[class-scheme]] file = …` in
  the manifest, inside the plugin's folder), offered as **From a plugin**
  in the LiDAR import and in Class colours.

**Another project into this one: towers in both, cables by id (owner, 2026-10-27)**

Found when combining several exports of neighbouring lines that share
junction towers and cables: every project-plugin import names its wire
file alike, so a second import's wire file "differed" and was left out
(its new cables missing) or kept both (the same cable id twice: the wire
library refused to load, and then the structure types were not found
either); towers in two exports came in twice or replaced each other.

- **A tower in both is kept once, as the project's** (its id, position,
  orientation and type), and its sets are asked for one by one
  (`ohl_io::import_into::SharedTower`, `SetChoice`; key
  `tower:<imported id>:set:<n>`): a set only in the import is **added**
  (default) or left out (its imported sections and jumpers with it); a
  set in both within the tolerance (the wire points in still air, world
  coordinates, so towers turned differently compare right) is the same
  attachment; a set in both at other places: **keep the project's**
  (default; the imported sections hang on it), **take the imported
  one** (replaces the project's wire point and strings of that set), or
  **add it under a new number** (a free number; the imported sections
  follow). A set added brings the imported wire point and the strings
  above it up to the structure points as the tower's own items (labels
  unique on the tower, string directions turned into the project
  tower's axes; a project point at the same place used). The imported
  tower is taken in effect (resolved with the import's own files). Its
  type file is not added when only towers in both use it. The window
  lists one line per tower, opened to its sets.
- **The same id further than the tolerance is another structure**,
  without asking: an id of its own (`T1-2`), a note with the distance;
  its type comes with it, under a new id and file when the project's
  type file of that name, or its type of that id, is another.
- Programs number alignment points, sections, jumpers and alignments
  alike in every file: **the same id elsewhere** (an alignment point
  beyond the tolerance, a section or jumper over other structures, an
  alignment of another route) is another object, kept apart under an id
  of its own (`sec1-2`), counted in a note. Before, an alignment point,
  structure, section or jumper of the same id was always a duplicate,
  kept as the project's by default: the import lost it. An alignment
  point at the same place alike but for its id is the same.
- **Cables by id, one of each in the project** (owner, the same day):
  the import's wire files are read cable by cable; one the project has
  with the same data is left out; a new one is added **to the project's
  wire file** (the first by name when it has several, said in a note),
  never as a second file of the same name; one of the project's ids with
  other data is a duplicate (`cable:<id>`): keep the project's (default)
  or the imported one under a new id (`<id>-2`, added likewise; the
  imported sections and jumpers name it). A project without a wire file
  gets the import's as it is. A cable the wire library would refuse
  (seen: a cable whose area exceeds its diameter's circle by rounding)
  is left out with a note, so the library keeps loading.
- **Structure type files by type id**; a type file whose name is taken
  by another type is added under a name of its own (`<id>.ohls`)
  without asking (keeping the project's would leave the imported
  structures' type missing). **Every structure's type must be in a type
  file of the project after the import**: the plan lists any that would
  not be (`ImportPlan::missing_types`), in the window before Import and
  on the command line. *Keep both* of a type file now gives the type a
  new id when the project has its id (two files of one id did not
  load).
- The window plans with the project line as stored (before: the line as
  resolved, so typed structures were written back merged with their
  types) and with the project's structure, insulator and wire files.
- **Command line**: `ohl project import-into FILE --into PROJECT.ohl
  --user U --plugin DIR [--option K=V] [--choose KEY=keep|imported|both]
  [--choose tower:T:set:N=add|leave|keep|imported|new] [--route
  ID=existing|new|ROUTE] [--tolerance M] [--dry-run]` prints the plan and
  writes it as one change; it says whether the wire library loads.
- Tests on synthetic data only (the example line split at its junction
  tower: a set only in the import, the same, at another place with each
  choice, a tower of the same id far away with its type, cables alike,
  new and differing, type files); the real exports were checked locally
  only.

**Plugins may suggest wire colours (owner, 2026-10-27)**

- The owner asked for a utility's wire colour table (by owner, voltage,
  frequency and phase) to come from its plugin. Plugins could not set or
  suggest colours: a new optional export `wire-colours` (extension point
  `wire-colours`), additive to interface version 0.3 like
  `sheet-templates` (plugins built before are unchanged). The host sends
  one group per circuit, phase and system with what the line knows (the
  system's owner, number, voltage, frequency, AC/DC) and the plugin's own
  column values; the plugin returns a colour or none per group.
- Applied only when asked (Line › Sections › Wire colours from a
  plugin…): shown first, for the sections without a colour of their own
  (default) or every section, written as the sections' own colours in one
  command. Nothing is coloured behind the user's back, and a project
  opened without the plugin keeps its colours.

**A plugin held below its asked limits says so when it fails (owner report, 2026-10-27)**

- Since the security review (2026-10-26, M6) a WebAssembly plugin whose
  manifest asks for more time or memory runs with the defaults (5 s,
  256 MiB) until the user allows it in the plugin manager. A large file
  through a project plugin then ran out of memory and failed as a bare
  "wasm `unreachable`" trap, which did not say what to do. Every plugin
  call that fails (trap or time limit) while the plugin is held below its
  manifest's `[limits]` now starts its error with the advice to allow it
  in the plugin manager. The limits themselves are unchanged.
- The command line `ohl` did not read what the user allowed (only the
  applications did), so a project plugin asking for more memory always
  failed there, even when allowed. It now reads the same shared plugin
  settings (the applications' data folder, found without the window
  library; a test keeps the folder equal to the applications').

**Version 0.0.2: the macOS download holds only the two apps (owner, 2026-10-27)**

- The v0.0.1 disk image also carried the command line, the examples,
  the licence and a read-me beside the apps. The owner wants the
  Applications folder kept clean: from 0.0.2 the disk image holds only
  OHL-Design, OHL-Structure and the Applications shortcut (the licence is
  inside each app). The `ohl` command line and the example line are a
  separate, optional download on the same release page.
- The repository's releases are immutable, so v0.0.1 stays as it was
  published and the slim build is released as v0.0.2 (workspace version
  0.0.2; the export samples carry the new version).

## 2026-10-26

**Performance review: long tables draw the rows in view (owner, 2026-10-26; items 6 and 7)**

- Display only; the results are unchanged. Redone on the tables as the
  UI reviews left them (units in the headers, numbers right-aligned, the
  value fields' look, the not-a-number usage first): the first version
  (on an older base) could not be merged.
- OHL-Structure: the load table (thousands of rows on a large tower,
  each copied every frame), the Members results (sorted every frame) and
  the Members and Joints sheets were laid out whole every frame. They
  now draw only the rows in view (`rows::Table`: one row height, the
  table taking the room of all its rows in its scroll area, the columns
  kept at their widest cell so far so they do not jump while
  scrolling); the load table reads its rows in place (no copy); the
  Members results are sorted once per run and case shown (the same
  order, `usage_order`: a usage that is not a number first). A member or
  joint picked elsewhere (the 3D view) is still brought into view: by
  its place in the table.
- OHL-Design: the lists of the wire tension, string and internal
  clearance checks, the zone findings, the vegetation spans and the bill
  of materials draw only the rows in view (`rows_in_view::Table`, its own
  copy of the helper; each list's spacing, headers and click-anywhere
  rows kept), so every row is listed (the lists' 5000-row cap is gone;
  the CSV export was always whole). The Tables page sorts on each row's
  cell and its number worked out once, not twice per comparison (the
  same order). Not done: keeping the Tables page's rows between frames
  (solved values arrive from background solves under the same model
  revision; it needs a count of solved cases first).
- Tests: the Members sheet of `type-02` draws its first member and not
  its last until that is picked, then the last and not the first; a
  5000-row list draws fewer than 30 rows and scrolls to row 4000.
- Measured (bench-fast profile, M1 Max under a load of about 20 from
  other builds, one whole harness frame, example `type-02`: 178 members,
  46 joints): Members sheet 1.4–1.6 ms → 0.35–0.6 ms, Joints sheet
  0.9–1.4 ms → 0.45–0.5 ms. A large lattice's frame no longer grows with
  its rows.

**Performance review: the time limits read a shared clock (owner, 2026-10-26)**

- The performance review of 2026-10-26 (owner-approved fixes, speed only:
  the results the same bit for bit) found the time budgets reading the
  system clock on every iteration of every catenary element solve.
- A run (`ohl_core::budget::Run`) now reads no clock without a time
  limit, and with one reads a shared counter that a background thread
  (`ohl-budget-clock`, parked while no timed run is alive) keeps from the
  clock once a millisecond: a time limit is noticed within about a
  millisecond (the tick and the thread's waking), early or late. The
  iterations, the iteration limit and the cancel token are exact; no
  iterate depends on the clock. The catenary element's 10 s backstop is
  gone (it could never trip before its 100 iterations); the enclosing
  wire solve keeps its time limit and cancel token. An iteration limit
  hit by an untimed run says no time ("stopped after reaching the limit
  of 100 iterations").
- Measured (M1 Max, shared with two other builds: load 60–100 on 10
  cores, so CPU time is the measure; invented 60-span line, 131 wind
  steps): the 131 solves 48.7–49.1 s CPU before, 46.9–48.3 s after
  (2–4 %); the synthetic example's solves (every calculation, every
  case, one core) 8.6 s before, 8.1 s after (6 %); again on a quiet
  machine, 7.0 s → 6.75 s (3.5 %). Less than the review estimated from
  its profile (half of the samples in the clock): here the clock reads
  cost a few per cent. Same results: the solves and the zone reports of
  both lines the same bit for bit (`{:?}` dumps compared).

**Performance review: the point checks pass far pieces over (owner, 2026-10-26)**

- With wind steps a point was tested against every state's piece of
  every span near it (the review: 33 µs a point with 131 states).
- The vertical clearance now has the lower bound the radial has: from
  the piece's height range (the bundle's lowest and highest
  subconductor, the slice's rise, the allowance; less 1 µm for the
  rounding of the point found on the wire), a piece that can neither
  give a smaller clearance nor one below the requirement is passed
  over before the exact work.
- The zone check's wire index keeps a cell's pieces in blocks by span
  (`WireSet::grouped`: a span's pieces in every wind state together),
  with the block's bounds: a point beyond the reach of a whole block
  (radial: the block's box less its largest radius and allowance;
  vertical: outside its plan box widened by the zone's width, or above
  its height bound) passes it over at once. Each piece of it would have
  been passed over by the same test, so nothing found changes, per wire
  either (a point beyond the reach is never recorded).
- Measured (CPU time, 60 spans, 131 wind states, 10⁵ points; the index
  built included): vertical 3.4 s → 1.8 s, radial 4.3 s → 3.0 s. Same
  results: the zone reports the same bit for bit; a test checks the
  grouped index against the reference check.
- Merged with the vertical clearance measured on the slope (r / cos β,
  physics review the same day): both bounds measure the bundle's extent
  and the allowance vertically too (the piece's own factor, or the
  block's steepest piece's), so they stay below the exact gap.

**Old names no longer read (owner, 2026-10-26; roadmap U12)**

Before the first release (v0.0.1) the compatibility paths for the names of
before 2026-10-18 are gone: the programs `ols`, `ols-cad` and
`ols-structure`; `import ols` (python/ols); `ols-plugin.toml` manifests
(only `ohl-plugin.toml` is read; the hint for sdk 0.2 plugins went with
it, they are refused as any other version); the `ols` file extensions
(`.ols`, `.ols.json`, `.olsw`, `.olss`, `.olsslc`, `-clearances.olsr`…)
and format ids (`ols-…`, OLS XML's namespace and `ols-line`, the
`ols-xml` importer id); `ols://` invitations; `OLS_BUILD_ID`,
`OLS_STRUCTURE_APP` and every other `OLS_…` variable (`ohl_core::env`
reads only `OHL_…`). `ohl_core::files` lost `legacy_name`, `is_format`,
`open_extensions`, `exists_by_either_name` and the `LEGACY_…` constants;
the tests of the old names (`ohl-cli` `older_names`, `legacy_name`,
`tests/data/before-ohl`) went, and the version-2 line files of
`ohl-line` (`tests/data/*-v2.ohl.json`) carry the new id (their version
is what they test). Kept on purpose, being on-disk or protocol names
rather than readers of old files: the table `ols_meta`, ALPN `ols/2`, the
storage ids `OLS CAD` / `OLS Structure`, the window storage keys
`ols-structure-…`, the web client's `ols.…` browser storage keys, the
ADRs (history). The owner's machine holds no files of the old names, so
no migration. No calculation changed (`ohl validate benchmarks`: 186
passed).

**A Roles page (owner, 2026-10-26)**

What a viewer, a designer and an admin may do is one table in the session
crate (`ROLE_ABILITIES`, each ability with its least role, plus a sentence
per role), shown by a new page **Roles** (View) with the user's own role
marked, and in the server guide. A test tries a sample of its rows against
the session's checks (the role below refused, the least one allowed).

**OHL-Design: UI review (owner: OHL-Design UI review, 2026-10-26)**

The owner approved the same review for OHL-Design as for OHL-Structure:
both programs look and behave alike, each with its own code (OHL-Design
and OHL-Structure do not depend on each other: `style.rs` and
`notices.rs` copied and adapted, no shared crate). No calculation, file
format or engine changed; every model edit is still one command (undo);
the UI settings below are the window's, kept between runs, not the
project's.

- **Look.** Value fields (numbers dragged or typed) have the inset fill
  and frame of a text field, buttons their raised fill
  (`style::field`); scroll bars always show; the text a step larger;
  **View › UI scale** (90 %–150 %, kept; a tick menu: it stays open).
- **Tables.** Units in the column headers ("H [kN]", "Sag [m]", "E [m]":
  a table column takes the unit its help gives after a comma), bare
  numbers right-aligned, headers in sentence case (a one-letter symbol
  stays). Done in Properties' weather case tables and the table pages;
  the check windows' result tables follow as they are reworked.
- **Messages.** Also a notice at the bottom right: information fades
  after 4 s, an error stays until its ×, a long one scrolls in a box of
  fixed height. An edit's message is a short sentence (*Section S1
  changed and recalculated.*, *Undone.*, *Saved.*), the whole text (log
  entry, sections and cases solved, time) on hover of the status line
  and of the notice; the status line stays.
- **Not solved.** Properties says once, above a weather case table, how
  many cases are not solved yet and why (only the cases shown are solved
  while editing), with **Solve all** (every case solved in the
  background); their cells show "—" (a case solved in which the section
  failed: "failed", the reason under the table as before).
- **Layers** (the Model panel): in groups — Ground & survey, Line,
  Results, Backgrounds —, each foldable, the folded ones kept between
  runs.
- **Cross-section panel**: its front view set up for the view's own
  shape (as drawn last), and set up again while untouched when the panel
  settles to another shape in its first frames (the ±width was cut off
  at the sides).
- **Clicks and the object's menu** (as OHL-Structure now). In the
  views (3D, plan, profile, cross-section) a left-click uses the tool
  held, with Select held it selects; a right-click selects what is under
  the pointer and opens its menu: Properties, Zoom to, the tools that
  apply (Edit…, Move…, Replace structure…, Add string…, Add jumper,
  Point or load on the span…, Measure the clearance, Link to a joint…,
  Add structure here…, Add alignment point here…), Use *tool* on it,
  Cross-section here (the profile's at its station: its own Select-only
  menu gone) and Delete…. A tool chosen there acts once, the tool held
  before stays held (one of several clicks stays held). Escape with
  nothing else to give up holds Select again. Nothing moves on a click or
  drag with Select: the structure screen's drag handles are dragged with
  the Move structure tool held (a structure itself moved only through
  the Move window, as before). The option "Tools act on left-click" is
  gone (it is now how clicks work; its kept setting is not read). Every
  text saying "right-click" for a tool now says "click". Not done:
  **Shift + click adds** — OHL-Design's selection holds one object; a
  selection of several needs the model of the selection, the views' highlight
  and Properties reworked (left for the owner to decide).
- **Check windows: Vertical and Radial clearances** (one window's code,
  the pattern for the others): the result first — a banner with the
  worst margin over every rule (clearance found less required), its span
  and weather case, red below a rule, green when every span is clear; a
  click on it or on a result row brings the views there —; the rules one
  table edited in place (the "Add a rule" form removed; **Add rule** adds
  a row; the preset choice stays); headers in sentence case with units;
  the table scrolls sideways. Every check window's explanations
  (`check_help::explain`) are one short line and a **(?)**, the whole
  text on hover. Run stays in the footer at the bottom left, its timing
  small and weak. The Clearances tab (wire clearances) is not reworked
  yet.
- **Add structure window**: the explanation under *Also on other points
  of this alignment* is one line and a (?); the structure preview's
  labels no longer overlap — each moved a line down or up when that is
  free, else left out and shown while the pointer is near its point.
- **Menus by task**: File, Edit, View, Line, Checks, Plans & sheets,
  Versions, Help. **Line** holds the former Terrain, Structures, Sections
  and Criteria menus as submenus (their contents unchanged); the Tools
  menu is **Edit › Tools**; the Plugins menu is **File › Plugins** (Plugin
  manager…, Python console); **Plans & sheets** holds Drawings… and the
  plans (Route, Situation, Phase plan…, Plan set…, out of File › Export)
  and the Bill of materials (out of File). Every action is still there,
  every shortcut unchanged; the user guide's menu table and every menu
  path in the guide, the tutorial and the program's texts follow (the
  decision log keeps the paths as they were).
- **Quick bar**: Undo ⟲, Redo ⟳, Save all 💾, Fit views ⛶ and Check wire
  clearances ✔ are icon buttons (their names, shortcuts and what they do
  on hover; their names for screen readers), set apart from the menus;
  other commands put there keep their names.
- **Dock as a panel**: a small 📌 in the window's title bar (left of its
  ×), no longer a row of its own; a docked panel's **Float** a small ⬈
  right of its title.
- **View › Layers** shows the layers in the same groups as the Model
  page.
- **OHL-Structure**: the 3D view's fit takes in the compass's angles
  written around the feet and centres the structure in the room above
  the hotbar ("270° left" and "180° back" were under it).

**Admins merge alone; connection limits are the host's parameters (owner, 2026-10-26, on the security fixes)**

- An admin merges a review into a protected branch alone: no approval by
  somebody else, and an open review too (the admin's merge is the
  approval). Designers keep the four eyes rule. Test.
- The connections at once (default 256) and the connections of one
  address not joined yet (default 8) are the host's parameters (`ohl
  serve --max-connections --max-per-address`, the Host window) and hold
  for loopback too, so a reverse proxy does not lift them. Join rate and
  wrong-code waits still leave loopback out. Test.
- Settings changes stay out of the oplog (owner: no); a settings file with
  an `allowed` table unreadable by older builds is accepted (pre-alpha).

**Fixes of the physics and code-quality reviews (owner approved, 2026-10-26)**

The owner approved fixing the findings of both reviews of 2026-10-26,
except wind directions other than across the line (later). What changed
and what was decided on the way:

- **Broken wire in every span** (physics B1). The per-component envelope
  of the breaks stays the loads shown (Structure loads page, CSV) and
  what the allowable-loads check (Method 2) takes — it checks each
  component on its own in both directions, so the envelope is on its safe
  side. It is **not** a bound of a member force combining the components,
  so the finite-element and pole checks now take each break as its own
  load and keep the worst usage, naming the span; OHL-Structure gets one
  line case per break, `<case> (span k broken)` (`CaseLoads::broken_each`,
  `CaseLoads::each_break`). Tests with the reviewer's example. Results
  change only for a load case with `span = "every"` (none of the
  benchmarks).
- **Ice load without a density** (B3). No longer called Conservative: a
  Simplification, not on the safe side for the wind, swing and clearances
  against lighter ice of the same weight (the reviewer's 28 % example in
  the user guide). The weather case table now says so for every case with
  wind and an ice load but no density, asking for one
  (`WeatherCase::ice_density_note`). No density value was picked by the
  program; glaze stays the default.
- **Jumpers in the wind** (B4). Decided: the safe side. Each jumper piece
  now takes the case's full pressure square to itself, as the spans do,
  pushed to the case's side of the line (before: cos² of the angle
  between the across-line wind and its normal, so a jumper across the line
  took almost none). No benchmark or test contradicted it; no benchmark
  result changed. Labelled Conservative in the jumpers' theory and the
  conventions.
- **Load factors on components** (C1). Documented, not redesigned (the
  load cases' V/T/L factors are the owner's): a Simplification with the
  reviewer's example in the structure analysis theory, the user guide and
  the load case library's help.
- **Vertical clearance on a sloping wire** (C2). Exact now: the surface
  radius and a bundle's subconductor offsets are measured vertically,
  r / cos β, capped at r plus the piece's own rise (a steep or vertical
  jumper piece). Ground, points and wire crossings; a level piece is
  unchanged bit for bit. Benchmarks: `clearance-surface-001`
  ground_vertical_m −2.9e-8 m, nothing else. The swing band keeps the
  radial take-off: now labelled Simplification (unsafe by
  r(1/cos β − 1)).
- **The pole's own weight in P-Delta** (E1) times the case's vertical
  factor (OHL-Structure: the case's self-weight factor), as the members'
  own weight; `ohl structures pole` (factored loads from a CSV, no case)
  keeps 1 (`check_pole_factored`).
- **Docs**: wirefe's stale "no ice on bodies without a density", the
  tension sections' opening (the ruling span is the idealisation; sheaves
  are the default in the blocks), the thermal page's "about a fifth" → "about
  a quarter" (24 %), and conventions.md: wind across the line is
  conservative for the transverse loads and side clearances only — no wind
  along or at an angle to the line is a stated **limitation** until wind
  directions come.
- **Code quality.** One usage classification (`ohl_structures_fe::check`:
  `classify_usage`, `UsageClass`, `NEAR_FULL`, `usage_order` with a NaN
  above everything) for the line's checks, both applications and the
  report (behaviour as before: NaN is over), re-exported by
  `ohl-structure-link` so the structure application keeps reaching the
  engines through the link only. The pole check refuses a load, wind or
  factor that is not finite with an error naming it (was a panic). A
  shared `ohl_core::units::parse_finite` for typed numbers: the structure
  edits refuse `nan`/`inf`/`1e400` (were written into the file) and a
  mistyped length (was 0 m); quantities with units refuse an overflow;
  LandXML points, `--area` and two OHL-Design table cells too. One
  placement rule (`ohl_line::links::feet`, `axes`, `to_structure_axes`,
  `from_structure_axes`) instead of the copies, and one `structure_bodies`
  (`ohl_clearance::internal`, now depending on `ohl-structures-fe`) for
  `ohl internal` and OHL-Design — every benchmark bit for bit the same.
  One plugin data folder constant (`ohl_plugins::STORAGE_ID`, still "OLS
  CAD": kept). Spotting refuses a step giving more than 1 000 000
  stations (a limit of the computation, not of the design). Dead code
  removed (`low_point_elevation`, `solve_jumper`, `sends_load_case`,
  `BrokenSpan::as_toml`, `SwingArc::line_at`); test-only helpers under
  `cfg(test)` (`profile_entity`, `StructureLoadCases::to_tables`,
  `Tool::icon`). Kept, for the owner: `Live::set_library` and
  `line_settings_put` (ohl-session, being changed in parallel; the first
  is described as a fix in the data-model review), `StructureFile::reload`
  and `tile_extents` (APIs meant for the applications) — these four
  removed the same day (owner: not needed) — and
  `wind_area_per_length` (in the user guide). The old names (`ols`
  programs, `import ols`, `ols-plugin.toml`, `.ols`, `OLS_BUILD_ID`) stay
  until the owner sets a date (roadmap U12). ARCHITECTURE.md now says what
  the crates' dependencies are (ohl-io on ohl-session, ohl-lidar on
  ohl-line, ohl-validation a client, ohl-sections a domain crate, ohl-app
  on three structure-application crates, the engines only through
  ohl-structure-link). Server tests through the public interface: a
  refused join, two clients on one object, a client gone mid-request (no
  server code changed). Skipped: a shared UI crate for the two
  applications (the owner decides later).

**Personal invitations no longer follow from `--code` (security review H2, owner-approved fix, 2026-10-26)**

Every access code used to be the first 6 bytes of SHA-256(secret, name),
with the secret the host's `--code`. A viewer holding the general
invitation could guess a short code offline from it and compute the
admin's personal invitation. Now only the general code comes from that
secret (unchanged: links made with `--code` stay valid); the personal
codes are HMAC-SHA-256 of a random 256-bit key over the name, 128 bits
(`ohl_server::Keys`). The key is made at every start, or kept in a key
file (`ohl serve --keys FILE`, mode 0600, made when missing; with `--code`
alone `PROJECT.ohl.server-key` beside the project), so personal
invitations still survive restarts when wanted. Personal invitations made
before stop working (hand out new ones). The application hosting keeps
both in memory as before (a key per run). Tests: RFC 4231 vector, the old
derivation refused, the same code after a restart with the key file, a
damaged key file refused.

**Reviews decided by designers, protected merges after somebody else's approval (security review M1, 2026-10-26)**

A viewer (e.g. a second connection of a designer joined with the general
invitation) could be named reviewer and approve. Now reviewers must be
designers or admins and never the author (refused at submit); deciding
needs that role still, and never by the author (admins included). Into a
protected branch, a review is merged only after an approval by a designer
or admin other than the one merging it (and other than the author); the
author may now merge their own approved review (a reviewer approves, the
author merges), so OHL-Design's Versions window enables Merge for the
author. Tests: a viewer and the author refused as reviewers, a demoted
reviewer decides nothing, the approver alone cannot merge into protected
main, the author can.

**The server keeps running for everyone (security review M2–M4, L3–L6, L8, 2026-10-26)**

- A job that panics inside the authority thread is caught: its client is
  refused ("an internal error of the server"), the session is read again
  from the project file, the server keeps running (test: a job made to
  panic, then edits and joins work).
- Joins: per address at most 12 a minute; after a wrong access code the
  address waits 1 s, doubling up to 300 s; at most 8 connections of one
  address not joined yet (QUIC and web each), 256 QUIC connections in all;
  loopback is not limited (the hosting application joins its own server).
  The copy of the project for joining clients is shared while nothing
  changed (`Snapshot`, removed when the last sender is done), at most 8
  exist at once, and a client that does not take its copy within 60 s +
  1 s per 256 KiB is dropped. At most 200 newcomers per run. A full copy
  is still made per join after any change (partly fixed: a delta transfer
  is the later fix).
- Presence: at most 10 a second (burst 20) per connection, the rest
  dropped; lock requests at most 20 a second (burst 64), the rest refused;
  the Peers list broadcast at most every 100 ms (latest wins).
- Messages are read as the bytes arrive (no allocation from the announced
  length), still at most 64 MiB.
- Locks: admins pass soft locks; an undo or redo is refused when the log
  entry it reverses touches an object another user holds.
- The copies for joining clients and the web scene's work folder are made
  0700 with random names.
- Names: invisible characters refused; a newcomer whose name's skeleton
  (lower case, full-width as ASCII, Cyrillic/Greek look-alikes as Latin,
  0→o, 1/i→l, spaces - _ . dropped) equals an existing user's is refused.
  A small table, not full Unicode confusables or NFKC.
Tests: gate limits per address, pacing, a forged look-alike newcomer
refused, an undo under another's lock refused, an admin's edit through.

**Project settings: rights, the server's log, no local paths (security review M5, 2026-10-26)**

`Op::SetSetting` needed only a designer. Now it also needs a designer
whose edits no grant limits, and an admin on a protected project line; a
value that is an absolute path is refused; the server logs who changed
which key. The copy a joining client receives keeps only the file name of
settings holding an absolute path (`app.weather`, the weather case file
used last; `Session::copy_for_others`). Settings stay outside the
operation log (not undoable, as before): logging them as commands would
change the session format — left to the owner.

**Plugins: limits and Python only with the user's consent; the store's limits (security review M6, L1, 2026-10-26)**

A plugin's manifest raised its own time and memory limits (up to 600 s and
4 GiB). Now `[limits]` above the defaults apply only as far as the user
allowed them for that plugin version in the plugin manager (asked once,
kept in the shared `settings.toml` under `allowed."name@version"`, asked
again for another version); until then the defaults apply. Python plugins
are not sandboxed: the plugin manager says so, they are off until allowed,
and their `[permissions]` section is no longer "checked" as if it bound
them. The WebAssembly store counts all of a plugin's memories together
against its memory limit (not each), and holds at most 64 instances, 16
memories, 64 tables of at most 2^20 elements. OHL-Structure keeps the
consents it reads (it writes the settings too). Tests: a greedy manifest
not raised without consent, raised with it, another version asked again;
the budget over two memories; Python asks to run unsandboxed; the shared
settings round trip with `allowed`; OHL-Design's Python scripts only once
allowed.

**Imports bounded: zip total, XML depth, xlsx references (security review M7, L2, L9, 2026-10-26)**

A zipped shapefile's members are read up to 2 GiB together (not 2 GiB
each) and at most 300 of them. The XML reader refuses elements nested
deeper than 256 ("elements nested more than 256 deep"), so a crafted file
cannot overflow the stack. OHL-Structure's xlsx reader (tests only)
refuses cell references that are not one to three capital letters up to
XFD and parts over 64 MiB. Tests for each.

**Project files opened defensively (security review L7, 2026-10-26)**

Every connection to a project file (the session's, the point-cloud
store's, the copies for clients) sets SQLite's `trusted_schema` off and
its defensive mode on: views and triggers in a crafted file run no
function unsafe for schemas, and the schema itself cannot be written
through SQL. Nothing in the file changes; every test of the session runs
with it.

**A usage that is not a number never passes (code review, 2026-10-26)**

The over / near / pass sorting of structure usages compared `usage > 1.0`,
so a NaN usage (which also governs, being sorted above every number) was
shown as a pass. Now it is "over" in the structure usage status, the
OHL-Structure results colours and its report. Test: a NaN FE usage gives
"over".

**Library file names checked wherever they are read (security review, 2026-10-26)**

A library file's kind and name become a path when the file is written
(e.g. another project line's libraries shown beside the line worked on).
Before, only the session's own helpers checked them: a raw put command
from a client or a crafted project file could carry a name such as
`../../x` and have it written outside the working folder. Now a put of a
library object with a kind or name that is not a plain file name is
refused, entries with such names are never handed out (nor committed into
a revision; restoring one takes such an object away), and the place that
writes the files refuses them too. Tests: a raw put refused, a crafted
entry skipped and removed on restore.

**OHL-Structure: UI review step 1 (owner: UI review step 1, 2026-10-26)**

The owner approved step 1 of a review of OHL-Structure's window. No
calculation, file format or engine changed; every edit of the model is
still one command (undo); the settings below are the window's, not the
file's.

- **Tables and fields.** The units are in the column headers ("x [m]",
  "Weight [N]", "Transverse [kN]"), the cells show the bare numbers,
  right-aligned; typing and dragging read as before. Headers are in
  sentence case (a one-letter symbol such as x, y, z, k stays). Value
  fields have the inset fill and frame of a text field, buttons the
  raised fill (the app's style, `style::field`). Scroll bars always show,
  so a table wider than its panel shows that it scrolls sideways. The
  tables are `egui::Grid`s: their columns size themselves to the widest
  cell (headers never clipped); keeping the first column in place while
  scrolling sideways would need the tables rewritten (`egui_extras`) and
  is not done. **View › UI scale** (90 %–150 %, kept between runs; the
  menu stays open on a tick) and the text a step larger by default.
- **Notices.** Opened alone, the load panels' permanent "No line
  values…" box is one short line, *No line values (opened on its own)*,
  the whole sentence on hover, closed with its ×. **Store in the
  structure file…** stays and says why it is greyed on hover. A message
  is also a notice at the bottom right: information fades after 4 s, an
  error stays until its × (long ones scroll in a box of fixed height);
  the status line at the bottom stays.
- **Start page.** Bigger buttons; **Open the example** (a copy of
  `examples/synthetic-line/Structures/type-02.ohls` opened from a
  temporary folder, not added to the recent files); the **Recent files**
  with their folders, a missing one marked *(missing)*, each removable
  with its ×.
- **3D view.** Buttons for the standard views — **Front** (looking along
  +y), **Side** (looking along −x, from the right of the line), **Top**
  (these three in parallel projection), **Iso** — and **Fit**; a legend
  of the usage colours while the members are coloured by usage; the long
  line of controls at the bottom replaced by **?** (the controls on
  hover).
- **Renamed**: the menu "In the line" and its panel "Line values" are
  both **Line loads** (the panel's and the area's internal names and the
  stored arrangement unchanged).
- **Header.** Above the panels, not a panel: the file's name (its path
  on hover, never printed), the level as a badge, *\* unsaved*, and a
  button **N to do** that brings the **To do** list (the former Level
  panel: what the next level lacks, then the advice, grouped as before)
  to the front; an item selects its object (joint, member, point) and
  shows its row, as before.

**OHL-Structure: tools, Inspector, a calmer arrangement (owner, 2026-10-26)**

Owner: "Just go ahead so that it works better for a user. Also maybe
remove the fact you can click on a joint and drag it straight away.
Maybe make a toolbar as is the case in the OHL design app."

- **No drag on click.** A click in the 3D view only selects (Shift +
  click: another). A joint or point moves only with the **Move** tool
  held (the same snap, one undo step per drag, as before); Escape lets
  go of a tool.
- **A hotbar like OHL-Design's**, in OHL-Structure's own code (the
  structure crates stay apart from `ohl-app`; the drawing copied): nine
  slots at the bottom of the 3D view, keys 1–9, an inventory of every
  tool (E) to arrange them by dragging, the arrangement and the slot held
  kept between runs. Tools: Select, Move, Add point, One string, Two
  strings to one wire point, Link, Add joint, Add member, Delete in the
  slots by default; Insert a joint on a member and Symmetry in the
  inventory. The tool held has a strip above the view with what a click
  does and its options (snap; a new string's length; where a joint is
  inserted; the symmetry and Make symmetric); the snap and the string
  length left the Geometry table's tools. The menus keep their entries
  (Edit › Move tool, Edit › Tools inventory added). Three icons added in
  the same style (`add-point`, `two-strings`, `insert-joint`;
  `assets/make_icons.py`).
- **Inspector panel** (right of the 3D view): the selected joint,
  member, attachment point (with its rigid link, link / unlink) or wire
  point (with its strings' lengths and its counterweight) with its
  fields, each change through the tables' edits (one undo step);
  several of one kind: the fields they share, a change written to each
  in one step; nothing selected: a one-line hint.
- **Default arrangement**: the 3D view large in the middle with its
  hotbar, the Inspector (and the To do list behind it) on the right, one
  area of tables as tabs below the view (as many as show their titles;
  the others open there from their menus), the header on top. No work
  modes. An arrangement kept by an older version gives way to it once
  (the arrangement now kept with a version, `ohl-structure-dock-version`);
  one that does not read gives the default, as before.

**Performance review: the radial ground check (owner, 2026-10-26)**

- The nearest triangle of a sample: a triangle whose bounding box is
  already no nearer than the nearest so far (with 1 µm to spare for
  rounding) is not worked out, and a triangle listed in several grid
  cells is visited once (`Terrain::triangles_near_indexed`). The same
  results bit for bit (the first of equal ones is kept as before).
- Owner-approved (2026-10-26), the zone check's ground clearances with
  wind steps: a span's positions in its states are checked in order on
  one thread, each with the span's smallest clearances so far as its
  bound, as a swing arc's lines already were
  (`checks::ground_check_grouped`); the spans run in parallel. What
  changes: only the clearance kept per wire position inside the check
  (a position that is not its span's lowest may keep none, or a larger
  one: its pieces and samples that can come neither below the span's
  lowest so far nor below the requirement are passed over). What does
  not: every count (each position's, so each span's), each span's
  lowest clearance, its state, its points (the first of equal ones in
  the states' order), so the report — findings, lowest per rule, counts,
  notes. Checked: the zone reports of an invented 24-span line (steps
  and arc, vertical and radial, a rule with spans below and counts) and
  `ohl zones` on the synthetic lines (vertical, radial, steps, arc) the
  same as before; a test compares each span's lowest and every count
  with the ungrouped check.
- Measured (CPU time, 60 spans, 131 wind states): radial 18.9 s → 2.1 s,
  vertical 5.6 s → 0.45 s; on the swing arc (3 states, the arcs' lines)
  radial 29.8 s → 23.1 s (the box test only).

**Performance review: a section's weather-independent parts worked out once (owner, 2026-10-26)**

- Every weather case and wind state of a section solved again what does
  not depend on the weather: its holders with the strain strings'
  estimated weights, its stringing reference, a curves cable's stringing
  state, and the final state after creep or after load (a still-air
  solve and sag-tension solves each).
- They are now kept in memos (`ohl_line::solve`'s `memo` module) keyed by
  a 128-bit fingerprint of their inputs' `Debug` text — every field, the
  numbers exactly: the section, its holders as the line gives them, the
  cable, the line's bimetallic model and condition cases (for the final
  state), its calculation options (for the stringing state), and the
  iteration limit. Any edit is another key. Failures are not kept. At
  most 4096 entries a memo, the least recently used first out.
  `forget_solves` drops them.
- Measured (CPU time): the synthetic example in every calculation
  (initial, creep, load, max sag; FE and RS; the curves cable; 6 cases
  and their wind steps) 7.6 s → 4.5 s; the invented 60-span line's 131
  wind states (initial, FE) 48.8 s → 45.9 s. The same results bit for
  bit (the `{:?}` dumps of every solve compared; a test solves each
  calculation from the memo and from scratch).

**Performance review: whole solves kept for the checks (owner, 2026-10-26)**

- Every check solved its weather cases and wind states again on every
  run, and each check its own: changing a rule, or the vegetation check
  after the clearance check, paid the solves again.
- `ohl_line::solve::solve_line_memo` (`solve_line_memo_default`: with the
  default exit clauses) keeps whole line solves keyed by a 128-bit
  fingerprint of the line (every field, the condition cases too), the
  wire library, the weather case and the iteration limit; the clearance
  zones (`zone::solve_states`), vegetation and safety volumes
  (`positions::solve_positions`), wire tensions and string checks take
  their solves from it. The internal clearances and the phase distance
  solve as OHL-Design's model does (`incremental::SolvedLine::solve`,
  its jumpers with it — not the same code as `solve_line_with`, so a
  memo of its own: `SolvedLine::solve_memo`, `Solved::new_memo`); those
  two share it. Never OHL-Design's cases updated edit by edit.
- Invalidation: an edit is another key, so an old solve is never taken
  for it; the solves of a line no longer among the last four solved are
  dropped, and beyond about 256 MB per memo the least recently used go
  first. A solve stopped by a time limit or a cancel is not kept.
- Measured: the invented 60-span line's swing arc after its wind steps
  (the same still and full-wind states) 1.01 s → 0.01 s CPU; a check run
  again with the line unchanged takes no solve at all. The same results
  bit for bit (the dumps and every `ohl` check output on the synthetic
  lines compared; a test checks the memo against a fresh solve and that
  an edit is solved again).

**Performance review: OHL-Structure's cases solved side by side (owner, 2026-10-26)**

- A structure's load cases are solved on the cores (each thread takes the
  next case; the results in the cases' order), in the application's run
  and in `analyse_cases`. Each case is solved on its own, so its result is
  the same whichever thread solves it; only its time ("took") is measured
  among the others.
- A load row's joint and member are found by id in a map made once per
  case (it was a search through every joint and member per row: 5500 rows
  × 3700 members), and each member's usage by member (it was a search
  through every usage per member: 13.7 million comparisons per case at
  3700 members); the first of an id, as the searches found it.
- Checked: OHL-Structure's headless tool's replies (`ohl structures
  analyse --json`) for every example structure the same as before
  (the times left out); a test checks the cases' order on any number of
  threads. Not measured on the 3700-member tower (private); the cases'
  part runs about as many times faster as there are cores, less what
  one case takes.

**Performance review: smaller solver items (owner, 2026-10-26)**

- Which sections run against the line (the wind's side swapped) is
  worked out once per line solve, the routes made once
  (`against_in`), not once per section and case.
- The sections and jumpers solved side by side (`jumper::in_parallel`):
  each thread takes the next item when done (fixed shares left threads
  idle on sections of very different lengths), the results in their
  order; inside a thread, nested calls run on it; and the checks solving
  wind states side by side give each line solve its share of the cores
  (`jumper::with_threads`), instead of every state's solve starting a
  thread per core (10 × 10 on 10 cores).
- Not done: the wire FE's allocations per Newton evaluation (the
  stiffness matrix is consumed by its LU factorisation: reusing it means
  reshaping the Newton loop, for about 5 % of a solve), and the
  jumpers' pull not copying the solved sections every pass (the jumper
  solver takes the sections by value throughout; the other agent works
  on the jumpers' wind there now).
- Measured (CPU time, quiet machine): the invented 60-span line's 131
  wind states 56.0 s → 46.0 s (the threads no longer multiplied), its
  wall time 5.8 s → 5.0 s. The same results bit for bit (the dumps of
  every solve and zone report compared).

**Performance review: LiDAR tiles decoded on other threads; build profiles (owner, 2026-10-26)**

- `ohl_lidar::for_each_record` (LAS/LAZ tiles: `ohl line --lidar`,
  `ohl zones` and the checks reading tiles) decodes the tiles on up to
  four threads, each tile's points handed over in batches of 65 536
  through a channel of its own (four batches waiting at most), and
  visits them here in the tiles' order and each file's order: the
  visitor sees exactly the sequence of a single reader (ties broken as
  before, the counts the same; a tile that does not read stops the visit
  after the tiles before it, as before). The gain is the decoding done
  beside the visiting: up to twice as fast when the visitor's work equals
  the decoding; tiles are not decoded far ahead (that would take memory
  without bound). Measured: 8 invented LAZ tiles of 10⁶ points read and
  hashed, 1.67 s → 1.37 s (the same points in the same order: their
  fingerprint compared); a test checks the order and the failure.
- Not done: the server encoding each broadcast once. It touches the
  events, the QUIC writer and the web clients' writer of `ohl-server`,
  which the security work changes now; left until that is merged.
- Owner-approved build profiles (Cargo.toml): `dev` with line tables
  only for the workspace and no debug information for the dependencies
  (backtraces keep their lines; the build folder smaller, links faster),
  and `bench-fast` (release optimisation, no LTO, 16 codegen units) for
  local timing runs; `release` unchanged. Documented in performance.md
  (Build profiles) and CONTRIBUTING.md.

## 2026-10-25

**Sheets: notes under the drawing, the key map whole (owner, 2026-10-25)**

- A sheet template's stringing table repeated the band row's values: a
  template may turn it off (`place = "off"`), and a note may stand under
  the drawing (`[[note]] from = "drawing"`, `at`'s y down from the
  drawing's lowest row) so the additional data follows the profile on
  every sheet instead of a fixed place over the plan's header.
- The key map is fitted with 2 mm of air inside its cell, so the
  sheet's box at the route's ends is drawn whole, not clipped.

**OHL-Structure kept fast on a large lattice (owner, 2026-10-25)**

- Owner: OHL-Structure slows down once the line's forces are in. Timed
  on a real 3700-joint-and-member tension tower with its 5500 load rows:
  60–130 ms a frame before, 5–8 ms after (the Members table about 22 ms:
  egui drawing every row).
- What a frame did again and again, now kept until the file changes
  (`StructureFile::revision`, unique among files; its text written once
  per revision): the geometry (twice), the level's list of what is
  missing (a search per joint through every member; now by maps and a
  sweep over the members by height, the same advice in the same order),
  a copy of the whole structure per panel (`structure_arc`), the 3D
  view's key (the whole text compared). The loads' and the results'
  arrows are built only when their key changes, the rows' points found by
  id. The Members and Joints tables find lengths, kinds and counts by id.

**A jumper between two wire ends: its window first (owner, 2026-10-25)**

- Owner: the window opens to set a length, "the minimum between the 2
  points by default but still always ask for a choice of method"; a sag
  may not solve in special cases (a large height difference). The jumper
  is put on Apply only, given by its length (the straight distance
  between its ends, rounded up to the cm), sag or rigid chosen there; a
  refused solve keeps the window open with the reason.

**Sections in the blocks over frictionless sheaves (owner, 2026-10-25 — reverses "not wanted" of 2026-10-22)**

- Owner: the default for every section in the blocks is now **equal
  tension along the wire over each sheave** (at each suspension clamp),
  in every weather case, the stringing state included; it replaces equal
  horizontal tension. The equal-H model stays as the option
  **simplified (equal H)**: Criteria › Calculation options, the line's
  `solver.blocks` (`sheaves` default, `equal_h`), kept in the project
  like the other solver options; line file version 10 when written; a
  file without it is calculated over sheaves.
- Owner: **the given stringing tension stays "H in the ruling span"**:
  the section's unstressed length is as before (every span at the given
  tension with the strings plumb, at the stringing temperature); the
  sheaves then share it out. Existing data keep their meaning — but at
  the stringing temperature the spans' H now differ slightly from the
  given one on unequal or inclined spans.
- **Every in-the-blocks result changes slightly.** On the synthetic
  example's first section (four spans, 25 kN at 15 °C, jumpers off, its
  point loads off): the spans' horizontal tensions by up to 159 N
  (0.6 %; at 15 °C −52 N to +59 N), their sags by up to 37 mm (15 °C,
  80 °C and 20 mm of ice at −5 °C), the clipping offsets (sagged at
  15 °C, clipped at 35 °C) by up to 0.8 mm. Clipped-in sections are not
  affected (but *clipping in* now takes the sheaves' lengths at
  stringing).
- The model: the wire runs over the sheave at the clamp point (its size
  left out) without friction; the tension at a span's end is the
  magnitude of its catenary's end force with its load (ice and wind
  included). One unknown, the first span's H, each next span's from the
  equal tension (a bracketed secant per span), the section's length the
  residual (a bracketed secant): `ohl_sagtension::sheaves`, used by the
  line's finite-element sections (`solve::over_sheaves`), the 2D
  sections (`TensionSection` and `NonlinearSection::solve_over_sheaves`)
  and so the sagging outputs (`ohl sagtension` `[sagging] blocks`, the
  section window's Sagging panel, each span's H shown). Over sheaves the
  strings swing **along the line** too, to the sheaves' resultant (each
  clamp's assembly solved alone under the wires' pull with its part
  along the line the sheaves'; a free clamp on the stiff wire would creep
  there over hundreds of passes).
- **Clipping offsets at the sagging temperature stay zero**: the wire is
  clipped in as it lies in the blocks (where it runs over the sheave); the
  "few millimetres" of the 2026-10-22 note are the differences against
  the equal-H model, not offsets of the sheave model.
- **The ruling-span method keeps one H** (decided here): it is itself the
  equal-H simplification; said in its doc and the theory.
- The estimated weights of strain assemblies given as geometry only stay
  estimated from the equal-H stringing state (the data keep their
  meaning); a section clipped in from the sheaves' lengths then differs
  from the one in the blocks by about 1 N in 25 kN on the example.
- **Friction later** (roadmap): it needs a tension ratio per sheave and
  the pulling end.
- Validation: `sag-004` (two inclined spans over a sheave, an independent
  reference with Newton on both H, `reference_sheaves.py`, and a hand
  check of \\(H_2 - H_1 = w\,\Delta z_{low}\\)); `sag-003`, `tbrs-002` and
  `tbrs-003` assume equal H and run with the simplified option (their
  references unchanged, no tolerance loosened); the other cases pass
  unchanged.

**A riser on a lyre pylon: a jumper given by its tension (owner, 2026-10-25)**

- Owner: the vertical conductor of a lyre pylon (a **riser**) is tensioned
  between top and bottom (dead-ended with a pull), and it is **a jumper**,
  not a new object. A jumper may now be given **by its tension**
  (`tension`, beside `length`, `sag`, `rigid`): N per subconductor, at its
  **higher end** (its `from` end when both are level; decided here), in
  still air at its temperature. Line file version 10 when used; OHL XML
  `<jumper tension>`; the jumper window's *tension* choice.
- The model (decided here): a taut wire of straight elastic bars (about
  1 m, 8 to 64) between its two wire points with its loads at the nodes,
  solved by the wire solver — any chord direction, vertical included (the
  catenary element cannot take a chord along its load). Its cut length
  is found from the tension in its design case (still air at its
  temperature, its objects on it, the jumpers tapping onto it not yet)
  and kept like a sag jumper's; in every weather case the length is kept
  (with the thermal change) and the tension follows the weather. A slack
  riser (a bar in compression) is refused for that case.
- The jumpers at its top and bottom **tap onto its points** (checked: a
  tap on a vertical jumper works; a point on a vertical riser is placed by
  its length or share, a distance along the line refused).
- Its pull goes into the **structure loads** where no section ends at its
  wire point: through its strings to their tops (shared equally, each
  with its weight), else on the wire point; its ends are the wire points
  as given (its strings not swung by it, a simplification). Clearances,
  internal clearances and drawings take it as a jumper.
- **Post insulators along the riser** (helping strings): not simple in
  this model (a taut wire with its supports would need the rod's
  constraints): left for later, a riser with helping strings refused.
- Validation: `riser-001` (a vertical riser of invented data: still air
  exact, the bottom's tension T − wL₀; in a wind the continuous elastic
  string by an independent shooting reference, `reference_riser.py`; the
  hand check \\(qL^2/8T_{mean}\\) with the bow's stretch raising the
  tension, 0.160 m against 0.236 m at the still-air tension).

**Background maps in the project (owner, 2026-10-25, roadmap O16)**

- Owner: "a map loaded into the project is fine"; shown on the plan, the
  sheets and the plans (not draped in 3D). A map is an image layer of its
  own kind (`background_layer`: orthophoto or map), imported as the
  orthophotos are (GeoTIFF or an image with a world file).
- The route and situation plans and the sheets' key map take the maps
  when the project has any, else every layer; the sheets' plan keeps the
  layers its template or the export names. The kind is chosen at import
  and turned with a click in Terrain › Backgrounds.

**Volumes on the strings in OHL-Structure (owner, 2026-10-25)**

- A structure type's string may carry its own volumes too
  (`[[string.shape]]` in the `.ohls`, version 7, values with units), each
  replacing its assembly's shape of the same name or added, for every
  structure of the type — the gap the second part of O6 left.

- Owner: OHL-Structure may use the core's placement through the link
  crate. `ohl_structure_link::string_volumes` places each string's shapes
  (its assembly's) on the string in still air, in the structure's own
  axes (orientation 0: x to the right of the line), with the line's
  `ohl_clearance::volumes::place`, and hands triangles to the view; the
  boundary stays the one crate. View › Volumes on the strings, off by
  default; the line's colours.

**A broken wire per span and set, in a load case (roadmap O9; owner, 2026-10-25)**

- Owner: "per span and set, in a load case"; the core broken-wire case
  approved 2026-10-19 ("the section solved with the wire gone"), for the
  design-code plugins to use.
- A load case names it: `[case.broken]` with `section` (id), `span`
  (from 1, as people count spans and the result's name shows them — the
  line file counts from 0) or `"every"`, and an optional `set`. **The set**
  is a section point's set: the wire of that set in that span, as the
  structures number their wire points; given, it is a check (the case
  fails when neither end of the span has it — a section renumbered or
  strung again), not a second way to name the wire. `.ohllc` version 3,
  written only when a case has one.
- **The model:** the section's unstressed lengths those of the whole
  section in the case (a section in the blocks rolled in the case, then
  held), no element in the broken span; its two clamps each a free end
  held by its strings alone — the section splits into two independent
  pieces, each joined to the other only through the structures (the
  owner's suggestion, the physical model: the adjacent string swings to
  the remaining tension). The other wires as they are. One finite-element
  model with the span left out (both pieces in one Newton solve), not two
  sections: the stretch after creep or load, the stress-strain curves and
  the rolling stay those of the whole section. A section end next to the
  break holds no wire: left out of the model, its strings' hanging
  weight and wind as its load. FE only (the ruling span refuses).
- **A bundle:** the whole phase broken (every subconductor in that span),
  marked conservative.
- **Every span, one at a time:** a solve per span, at every structure
  point the largest of each component (by size of the factored value,
  sign kept) with the span giving it (`AttachmentLoad::broken_span`, the
  loads CSV's last column); conservative, marked.
- **Left out, marked:** the dynamic impact (the case's longitudinal
  factor is the place for a code's), the pieces on the ground, the
  objects on the broken span; a TAK's broken section solved with its
  partners as whole; jumpers keep their loads of the whole line. Refused:
  a spacer or a jumper on a span point of the broken span.
- **To OHL-Structure:** a line result like the weather cases' — Criteria
  › Loads for structures › **Broken-wire cases sent** ticks the line's
  load cases with a broken wire (`structure_loads.broken_cases`, line file
  9); each is sent as its weather case with that wire broken, unfactored,
  in the menu's wire condition, named `<weather case> — S3 span 2
  broken` (a wind case twice, `… — wind from the left`), after the
  weather cases; its factors are the structure load case's (one place for
  them, as decided for the line results). Chosen over sending the load
  case itself, so that the line still sends forces only.
- Validation `brokenwire-001`…`003` (benchmarks/wirefe/broken_wire.toml,
  an independent reference script and the hand check
  \\( \tan\varphi = H / (V + W/2) \\)); the solver agrees to 10⁻⁷.
- No plugin under `local-data/` changed: the format for them is in Design
  codes (a `[case.broken]` table in the `.ohllc` text they write; the
  plugin interface unchanged).

**Local winds per span: a factor, for every wind case (roadmap O15; owner, 2026-10-25)**

- Owner: "a factor, for every wind case". A section's spans may carry a
  factor on the weather case's wind pressure (`wind_factors`, `{span,
  factor}`, span 0 = first as span points count; factor 0 to 10; a span
  not listed 1). Kept with the line (line file version 9, written only
  when used; OHL XML `<wind_factor span factor>`; XSD).
- Applied wherever the case's wind acts on the wires (found by grep of
  every `wind_pressure` and `wire_load` use): the spans' load in the
  finite elements (each span's case scaled: the blowout, the tensions,
  the objects' wind), the strings (the larger factor of their two spans,
  conservative), the rolling of a section in the blocks (each span its
  own wind), the spacers (their span's), the jumpers and their support
  strings in the structure loads (the largest at their structures,
  conservative), the ruling span and the permanent stretch after creep or
  load (the section's largest, conservative — one tension, one stretch
  per section), the wind span of the allowable-spans check (each half
  span times its factor); and through the solver every check: the
  clearances' blowout and swing arc, the distance under unequal wind
  (each level times each span's factor), the structure loads, the string
  and tension checks, galloping (on the solved sag). With no factor other
  than 1 everything is bit for bit as before (tested).
- **Not scaled, said:** the structures' own wind (bands, member wind,
  pole) stays the load case's (`[case.structure]` or a wind study per
  structure give a structure its own); a wind study's pressure (the
  structure override) is taken as the case's, the spans' factors on it.
  **Simplification, marked:** the same factor on the ice-with-wind cases
  as on the wind alone, no direction of its own (to decide later: a
  factor per case, a direction).
- **Edited:** the Sections table's **local winds** column (`2–4: 1.2;
  6: 0.7`, spans from 1, runs; one command on Apply), a span's Properties
  (**Local wind ×** factor on spans *a* to *b*, **Set**: a run of spans
  at once), the session command `set_wind_factor` (`ohl project apply`);
  each one undo step. The tables have no multi-row typing; a run of
  spans in one cell or in the span's Properties sets many spans at once.
- Validation `localwind-001`…`004` (benchmarks/wirefe/local_wind.toml,
  the elastic catenary in the tilted plane of the load, independent
  script); the solver agrees to 10⁻⁷.

**Small usability fixes found by the tutorial (2026-10-25)**

- String wire: a new section takes the circuit and phase of the section
  it continues (the same wire point, or one hanging from the same
  structure point — the other side of a tension structure), else the
  circuit's next phase at that structure (the first label not strung
  there; all strung, the number after the last). Before, the first
  section's phase was copied and had to be changed every time.
- A tool window that would reach past the bottom or right of the screen
  is moved back onto it in the frames after it opens; dragged later, it
  may still go anywhere.
- The plan view writes the cursor's easting and northing in its corner.
- The left column of the default layout is wider (28 % of the middle
  instead of 20 %): Next steps' titles no longer break every few
  letters. A layout arranged before is kept.

**Materials typed in OHL-Structure (roadmap U5, 2026-10-25)**

- Design › Cross-sections & materials › Cross-section library lists the
  materials and types them as it does the cross-sections (new, edit,
  duplicate, delete; one undo step each), checked by the section library
  itself.
- A material renamed takes along the cross-sections of its file naming
  it and the file's `default_material`; one used in its file is not
  deleted (other files' cross-sections: the library reports the material
  missing, as before).
- Partial factors typed as `name = value, …` (an inline table).

**Plan sets: one PDF, `ohl drawing --set` (owner, 2026-10-25)**

- Roadmap O14, the rest of item 3: a plan set also as **one PDF**
  (the Plan set window's tick "also as one PDF", `ohl drawing --set-pdf`)
  and written from the command line (`ohl drawing --set FILE` or
  `--set PLUGIN/NAME`).
- What goes into the one PDF: every sheet of the set's drawings in the
  set's order, each drawing for every route (the plan-and-profile
  sheets of every route, then each plan of every route), named
  `<set name>.pdf` in the output folder, written besides the files of
  the formats chosen. Only sheets: a plan-and-profile drawing without
  paper (one continuous drawing) is left out, and the result says so.
  The pages are the ones drawn for the files (kept, not drawn again),
  and kept only when the one PDF is asked for (they hold the
  orthophotos).
- Naming of a kind given twice (e.g. situation plans at two scales):
  the first drawing of a kind keeps its usual names, the second gets
  `-2` after the route's name, the third `-3` … (`main-2-sheet-01.svg`,
  `main-2.pdf`, `main-situation-plan-2.pdf`). Chosen over the scale in
  the name: a set with each kind once writes exactly the files of a
  single export, and two drawings of a kind may differ in paper or
  template rather than scale.
- On the command line the line is solved once for the whole set; each
  drawing's template, paper and scales go over the command's own
  options; the set's plans write no plan-and-profile sheets. A set's
  template name is looked for as in the app: the set's plugin's first,
  else another plugin's of that name, else as `--sheet-template` reads
  it. No file format changed.

**Another project imported into an existing one (owner, 2026-10-25)**

- Owner: "import other lines or project in an existing project. It should
  then ask if it's the same branch, the same route and offer a merge
  conflict help if things appear twice." File › Import › **Another
  project into this one…**, for any project plugin (the reader gives a
  line and library files; nothing in the plugins changed).
- The branch: the project line worked on, or a new project line made from
  it (then Versions › Bring in changes). The routes: each imported route
  is one of the project's or new; the same id is the same route by
  default.
- What appears twice: the same id with other values, or the same place —
  alignment points and structures within 1 m in plan (adjustable),
  sections over the same structures and sets, jumpers between the same
  ends — and library files of the same name. Settled one by one: keep the
  project's (default), take the imported one (under the project's id),
  keep both (the imported one renamed `id (2)`).
- The project line's own merge (Bring in changes) is not used for this:
  an import has no common ancestor with the project, so it would find no
  conflict and an imported object of the same id would silently replace
  the project's.
- Ids are changed in a scratch session with the project's own rename
  commands (references follow as in a hand rename); written as library
  files then one `ReplaceLine`. Coordinate systems must match.

**Kept measurements and notes: labels by the label distance, faster (owner, 2026-10-25)**

- 3D: a kept measurement's label and a note's text are written within
  the 3D label distance of the eye (Display options, default 1.5 km), as
  the other 3D labels; beyond it only the line and its ends, the note's
  point and leader are drawn (still clicked there with the Edit tool; a
  note's text only where it is written). Before, every kept label was
  written at any distance.
- In every view the kept ones' labels no longer overlap: what is being
  measured and the point under the cursor are written first, then the
  kept ones — in 3D the nearest first —, a label that would overlap one
  written is left out (its line or point still drawn). A display choice:
  no value changes.
- Others' measurements hidden (the tick): they are no longer computed
  again (a kept clearance sweeps its span); ticked again, they are.
- The views' overlay of the kept ones is made once when they change, not
  three times a frame, and only for the views that draw it.
- Follow and snap: with the point under the cursor, the points read and
  the case unchanged (a frame asked for by something else), the
  measurement found for it is taken again instead of being searched and
  measured again (the same value); the alignments are no longer copied
  every frame.
- Timings (Development › Performance, *Kept measurements and notes*):
  300 kept, others' hidden: computed again 5.4 → 3.4 ms; the overlay
  233 → 2 µs a frame; from 2 km 1 000 → 600 shapes drawn.

**Structure load cases: line forces and loads on the structure itself,
built (within the agreed spec, 2026-10-25)**

Built as the [spec](structure-load-cases-spec.md) says (phases 2 and 3);
decided within it:

- **Which self weight wins**: `[load_case.structure] self_weight`, when
  given, is the case's; the case's older `self_weight` must then be 1
  (else refused: "given twice"). Convert and the editor move it there.
- **The self weight has no rows** in the load table: the solve puts each
  member's weight × its dead-load factor × the case's factor on the
  members, as before (rows of it would count it twice).
- **Row numbers by part** (column `load`), so a value typed over a row
  keeps matching when another part changes: older loads 1–999, line
  forces 1001…, wind 2001…, ice 3001…, point loads 4001… (at most 999 of
  each, checked). Shown as "line 1", "wind 2", … in the table.
- **A point turned off keeps its row** (no force, noted "off"): a value
  typed over it stays on the same row whether the point is off or on.
- **Line forces act at the points carrying a wire** (a set) or given a
  value by the line in that result (only those of the row's sets when it
  names some); a value missing: the row says which.
- **The body wind's row per member is the member wind's full force**
  (normal to the member, as the line's checks apply it), not only its
  component along the wind.
- **Several ice loads**: each weighs on the bare outline; the widening
  for the wind is the sum of their 2 t. Faces, solidity and shielding
  stay those of the bare members (said in the theory page).
- **Extension and pole**: the body wind and the ice act on an
  extension's own members as on the body; the body wind (pressure ×
  factor from its direction) is also the pole's wind; the ice is not on
  the pole (said in the case's notes).
- **The recipes' CSV holds the older form only**: a case with the parts
  is exported as `.ohlslc` (said when CSV is asked).
- **Take the line's load cases as they are** makes cases of the new
  form: per line result, its line forces with every factor 1 and, with
  members, the wind on the body at the pressure of its weather case from
  its side.
- **Mirror wind** swaps "left" and "right" in the name, description,
  line results and body wind; the older loads and the points off stay; a
  case of the mirrored name there already is not replaced.
- The editor is the **Load case editor** panel (Load cases menu).

**A plugin's presets edited in the project (owner, 2026-10-25)**

- Plugin manager › **Presets…** on a plugin: a **Presets** tab (its rule
  presets, their weather cases and its sheet templates, value by value,
  editable) and a **Changes** tab (each value changed: the plugin's, the
  project's, its status, a note; CSV; reset) — "to ensure the tests are
  done properly".
- Kept **in the project** (owner's choice), not in the plugin: library
  kind `plugin-preset-edits`, one file per plugin, one undoable change
  each. Taken wherever the presets are read (the rule presets every
  window offers, the plugin's sheet templates); rule files loaded before
  keep their values.
- A value is found by its path (keys; a rule by its name); a newer plugin
  whose value differs from the one the change was made from: the change
  still applied and flagged; a path gone: not applied, flagged. A preset
  the changes make invalid is taken as the plugin gives it, said.
- A library write through `project_op` counts as a library change (what
  reads the libraries reads again).

**LeV weather groups and the wind study per pylon (owner, 2026-10-25)**

- The lev-ch plugin's rule presets write weather case groups with their
  cases: largest sag (40 °C; 0 °C + 20 N/m), wind deflection (10 °C wind),
  conductors–structure at rest (a new case, 10 °C without wind — the
  owner's reading of the LeV, "defined in the LeV is 10 °C no wind"),
  conductors–structure short approach (10 °C wind), distance in the span,
  ground and objects; a utility preset gets a group of its own cases.
  LeV 0 °C without additional load keeps no group ("leave it for now").
- A wind study is set per pylon only ("only through the pylon value"):
  the plugin's Structures column **wind study Pa**; the short-approach
  group (650 Pa by the altitude zone) names it (`structure_wind`) with the
  least 300 Pa (`structure_wind_min`, ESTI 240 Art. 33). With the structure
  override ticked, the internal clearances solve the group's cases again at
  each value and take those results at the pylons that have it; under
  300 Pa taken as 300 with a warning; pylons without a value keep 650.
- A preset's groups join the project's case file with its cases (mapped
  to the project's names); a group name the file has is kept as is.
- Annex 6's distances C1–C4 are not in the documents at hand: no
  internal-clearance rule preset is generated from memory.
- The design-code cases (loads) have no groups: the plugin SDK carries
  none, and the core is not changed for it.

**Internal clearances, first part: the computation (owner, 2026-10-25)**

- Measured at each structure at level 2 or 3: the conductors from their
  clamps out to a rule's `reach` (default 20 m, in plan along the solved
  wire), the jumpers at the structure and the strings' wire points, all
  as solved in the case (the swing included); level 1 (no body): listed
  as not checked. Earthed wires are not checked.
- Against every member of the body: to its surface where its solid is
  known (the drawn profile, a pole's outline), else to its centre line —
  the member's size left out, on the unsafe side, said per structure and
  per row. A wire end inside a closed solid counts as 0 (into it).
- Exact on the solved polylines (segment–segment, segment–triangle), to
  the wire's surface (radius and polyline bound taken off:
  conservative); a string's wire point is a point (fittings not counted:
  on the unsafe side). The volumes on the strings: the next entry.
- Rules: `[[internal_rule]]` in the rule file (`name`, `cases`,
  `voltages`, `distance`, `conductors`, `jumpers`, `string_ends`,
  `reach`), `checks = ["internal"]`; the structure and section overrides
  apply. `ohl internal` runs it; the desktop window is written apart.

**Internal clearances, the volumes on the strings (owner, 2026-10-25; details confirmed 2026-10-19)**

- The shapes belong to the insulator assembly (`.ohli` version 4,
  `[[shape]]`: sphere, cylinder, box, cone; grounded, electrified,
  insulated; position along the string from its top as a fraction or in
  metres, sizes, an offset across and along), so every string of that
  type has them. **A structure overrides one by name** in the style of the
  string overrides: its own string's `shapes` (line file version 8) — each
  replaces the assembly's shape of that name or is added; one overridden
  as insulated is left out (no removal key: minimal).
- **Freedoms** (`swing_across`, `swing_along`, `twist`: free or blocked)
  set the shapes' **orientation only**; their positions follow the solved
  string (its top and wire point give the axis). A blocked swing keeps the
  still-air angle in that plane; a blocked twist keeps the structure's
  across axis, a free one takes the across direction of the conductor at
  the wire point (as solved). A strain string's shapes always follow its
  axis (the conductor's pull). Approximate, said in the theory page: the
  solve's string itself swings as it does.
- **Several strings in one assembly** (`strings` 1 to 4, `arrangement`
  across / along / triangle, `spacing`): the line keeps one string on their
  middle (the solve unchanged); each string's copy of a shape is offset by
  the arrangement, parallel to the modelled string; a `shared` shape (the
  yoke plate) once on the axis. Approximate: the strings' own tops and the
  yoke's linkage are not modelled.
- **The check**: each electrified shape a part (`Part::Shape`); every part
  (conductors, jumpers, string ends, shapes) against the body and the
  grounded shapes of every string at the structure; electrified against
  electrified and grounded against the body not measured; insulated shapes
  never targets. Distances by GJK on the convex solids, reported as its
  lower bound within 1 nm (conservative). A rule's `shapes` (default true)
  turns both off; `distance_wind` gives another distance in the cases with
  wind (a wind pressure above 0), the owner's 2026-10-19 rule.
- **Display**: Structures › Display options › Volumes on the strings (3D),
  off by default, drawn on the solved strings within the solids distance.
  **Editing**: Structures › String shapes (a table, one Apply one undoable
  change) and the String library's freedom and string columns. Not drawn
  in OHL-Structure: its view would need the core's placement
  (`ohl-clearance`) across the application's one boundary — left for a
  decision.

**Weather case groups of structures and sections; the overrides (owner, 2026-10-25)**

- First a structure's single group and a maximum wind pressure from a wind
  study were proposed; the owner preferred, since some checks need the
  full wind: a **list of groups** on each structure and each section (the
  snow load varies along the line), and in **every check two ticks**, the
  **structure override** and the **section override** — ticked, a result
  takes only the check's cases that are also in its object's groups.
- A result at a structure (strings, structure loads) takes its
  structure's groups; on a span, its section's and both its ends'
  structures' (a span is at both); string results both their structure's
  and their section's. Objects without groups: every case of the check.
- Applied where each check takes a case for an object, before any worst
  value over cases (clearances: each span's wire per state and its swing
  arcs; vegetation and safety volumes: the positions of each span; the
  distance between conductors: the conductors of a case, the same at
  every unequal-wind level; galloping: the spans of its case; tensions
  and strings: their rows; structure loads: each load case's weather at
  each structure). The cases are still solved for the whole line.
- The structure loads take the structure override only (their loads are
  summed per structure from every section).
- ESTI Weisung 240 (Art. 33): a wind study proving lower pressures lets
  the wind for the conductors' distances to the structures be reduced to
  no less than 300 N/m², per EN 50341-1 — a group of such cases on the
  structures concerned; the bound is for the LeV plugin to check, not the
  core.
- The ticks are kept in the rule files (owner, 2026-10-25), as
  `structure_override` / `section_override` in `[check]`: the vertical and
  radial clearance windows, wire tensions and string checks. The checks
  without a rule file (vegetation, safety volumes, distance between
  conductors, galloping, structure loads) keep them for the session. The
  command line does not apply them yet.

**Measuring tools, phase 3: the wire tools (owner, 2026-10-25)**

- Wire to wire (T5): the closest approach of the two picked spans as
  solved, every segment pair (the clearance crate's public segment
  geometry; the unequal-wind check's own pruned code stays its own), to
  the surfaces (the nearest subconductors at the points found).
- Clearance at a point (T4): vertical and shortest to the ground surface
  the checks use; the shortest searched exactly over every triangle that
  can be nearer than the ground straight under the point (that bounds
  it). The sweep along the span runs on the click only (87 → 23 ms on a
  real span with the search narrowed to the best so far); the value
  under the cursor is the point's own. The sweep is at the solved points,
  bundles as one wire there (simplification, said).
- Wire to object (T6): to a structure's drawn lines (members, else stick
  drawing; strings) or a point. Background drawings cannot be picked yet.
- Every kept one recomputes from its anchors (the whole span searched
  again).

**Weather case groups; Loads for structures explained (owner, 2026-10-25)**

- Groups (a name, a description, cases by name) live in the weather case
  file beside the cases (owner's choice), `[[group]]`; made in Criteria ›
  Weather case groups…, saved as one change of the file.
- Picked in Loads for structures and in the checks' case lists (owner's
  choice; the display cases not): a group's button adds its cases.
  Galloping takes one case (no group).
- A case renamed in the table is renamed in the groups; a name no case
  has is kept and shown, not refused (a file edited elsewhere stays
  readable).
- Loads for structures now says where the load cases come from (the
  line's load case file and its name) and gives each case's weather case,
  condition, factors and description on hover, each weather case's
  values; "weather cases on their own" explained (every factor 1, the
  wires as strung). The owner had asked what a load case of an imported
  project was and where it came from.

**The Edit tool opens the line's structure in OHL-Structure (owner, 2026-10-25)**

- The Edit tool on a typed structure opens its type in the structure page
  as before; that page now remembers the structure it came from, and its
  button opens **that structure with the line's loads** (as Open in
  OHL-Structure… in Properties), not the family's type alone, so its
  load cases can be seen and edited. The type alone is a smaller button
  beside it; the page opened otherwise is unchanged.

**Measuring tools in use (owner, 2026-10-25)**

- The Measurements panel and the Follow and snap panel are dockable as
  Properties is; closing Follow and snap's tab puts the tool away.
- A measuring tool's click that opens the panel, a note's window or a
  kept one's window frees a locked mouse (first person).
- The Edit tool on a kept measurement or note (hit on screen within 8
  pixels) opens it: its label/text, Save, Delete.
- Follow and snap: a right-click on nothing takes the selected object
  (a span point's or spread's span, a string's or point's structure).
- The movement keys move the 3D camera only while the mouse is locked
  (first-person scheme); otherwise only over the view with no field being
  typed in — the camera moved while typing in a table.

**Follow and snap (T11) built (owner, 2026-10-25)**

- Objects followed: a span's wire, a jumper, a structure (its members or
  stick drawing and strings). The object's point is the one clicked on it,
  else the nearest in plan to the point under the cursor.
- The points: the stored ones of the ticked classes in the object's box
  grown by the radius, read once in the background into 2 m plan cells;
  each move an exact ring search (stops when no farther ring can be
  nearer). N: every point against the object's lines (boxes of 8
  segments skip the far ones), exact.
- Default classes: those shown but the line's own (roles conductor,
  shield_wire, insulator, structure) and noise — found on a real line,
  where a wire snapped to its own insulator points.
- A kept one keeps its LiDAR point (spec question 2, searching again after
  a change, still open). A structure point is kept as a new anchor
  `on_structure` (offset from its base).
- Real line, a span in the middle: 23–63 ms to read, 0.03–0.3 ms a cursor
  move, 12–27 ms for N.

**Measuring tools, phase 2: annotations (owner: "go ahead", 2026-10-25)**

- A new object kind `annotation` in the project, apart from the line (as
  reports): not in the line, its hash, the line file or the checks' line
  state; shared; ids `user-n`.
- Deleted with its object in the session itself, after any command
  (tools, tables, the command line, the server alike): an anchor whose
  object the command removed deletes the annotation in the same log
  entry; undo restores both. Only object anchors count; a LiDAR or free
  point stays.
- Picks and kept annotations share one path: a click becomes an anchor,
  the anchor gives the point (so a kept value is computed exactly as it
  was measured). A wire's anchor is its share of the span's length in the
  case measured.
- Measurements = dimensions, notes = notes; the two ticks hide others'
  of each (a user's view preference). Leaders are notes with a fixed
  short leader for now.

**Measuring tools, phase 1 built (owner: "please start", 2026-10-25)**

- Three tools in a new Measure theme: Distance, Height above ground,
  Stations; read-only (no editing rights needed). The point under the
  cursor is found every frame the pointer moves while one is held (the
  same search as the tool click), named and measured live; the status
  bar gives its station and offset on the nearest alignment (shown only
  while a measuring tool is held — spec question 9 still open).
- A pick: a wire where clicked (case shown) to its surface; a structure
  its base; an attachment point as placed (still air, said so); LiDAR,
  survey and alignment points; else the ground (plan, 3D) or the point
  on the axis at the cursor's height (profile, one alignment shown —
  several joined: no free point, their stations do not map back).
- Stations from the nearest alignment by offset (the alignments' centre
  lines kept per line version).
- The measurements are the session's (the panel: copy, CSV); keeping them
  is phase 2. Key M is Move's: the measuring tools have no key yet.
- Cost on a real line with its LiDAR (3D view, pointer moving): the
  median frame unchanged with Distance held (0.6 ms, as with Select).

**Measuring tools and annotations: the spec's first decisions (owner, 2026-10-25)**

- Draft in [measuring-tools-spec.md](measuring-tools-spec.md) (a
  proposal; nothing built). Decided: annotations stored apart from the
  line (no effect on its hash, not in the line file); wire distances to
  the surface by default; an annotation deleted with its object (one
  undo); shared by everyone, others' annotations and measurements hidden
  by ticks in the Display options.
- A tool added on the owner's request (T11): select an object, choose
  LiDAR classes, the measurement follows the cursor along the object
  snapping to the nearest point of those classes; a click locks it as an
  annotation.
- The main goal of these tools: responsive, fast and intuitive; never by
  coarsening computed values.

**The phase plan redrawn after a utility's (owner, 2026-10-25)**

- The owner gave a utility's phase plan as the model: every wire a
  track along the page at its place across the line, coloured by phase,
  dashed by circuit; each run of structures with the same arrangement
  one column ("first–last") with the structure seen from above (arms
  across the tracks, a dot per wire, the earth at its foot); tracks
  crossing where the phases change places; circuits leaving or joining
  turning away with an arrow and their name; at the line's ends each
  track's phase and its circuit's name; the table of the circuits.
  It replaces the figures drawn only at the changes (owner: replace).
- The table's owner and number are new optional fields of the
  electrical system (`owner`, `number`; Tables › Systems, the line file,
  OHL XML): owner's choice, on the system rather than the circuit (one
  system per circuit in practice).
- The ends show an arrow and the circuit's name per circuit for now; the
  substations' portals (with their phase labels) come later (owner).
- Phase colours on this plan: each circuit's phase labels in order red,
  green, blue (the usual phase-plan colours), not the line's display
  colours; earth wires dark grey.
- Jumpers are not tracks (a dead end's jumper may reach metres along the
  line); a strain structure's two clamps of one phase count once.
- A wire is placed by where its strings hang on the structure (the top of
  its chain: a V's legs' middle), not by its clamp (owner, 2026-10-25: a V
  with a short string below looked like another arm; a real line's phase
  plan went from 5 sheets to 2). The conductors at the line's ends only
  where they fit (always in the table).
- Legend labels: `phase_unchanged` gone; `phase_circuits`,
  `phase_owner`, `phase_line_type`, `phase_circuit`, `phase_conductor`,
  `phase_number` added.
- Then (owner: "add what you can"): the conductors per circuit (phases
  × subconductors × cable of its first section; no cross-section, the
  drawing has no cable library) at the line's ends and in the table; the
  table lists the circuits of its sheet only; a sheet's tracks enter at
  the places the sheet before left them (a change at the break drawn on
  the new sheet); a circuit ending where the next begins: their arrows
  and names apart.

**The plans' weather cases (owner, 2026-10-25)**

- The phase plan is a schematic: it is drawn in the case shown, never
  the template's or the Drawings window's cases per circuit, so it never
  waits for a case to be solved.
- The route and situation plans and the sheets of a plan set keep their
  cases; Export › Plans now asks for them to be solved while the window
  is open (before, only the Drawings window did, and the export waited
  for a case nothing solved), and an Export asked meanwhile starts once
  they are.

## 2026-10-24

**The ground rule of the sheets chosen (owner, 2026-10-24)**

- Export › Drawings…: the ground rule the clearance line and the lines
  below the lowest conductor take is chosen by name (default as before:
  for each circuit the largest of its voltage); kept with the project's
  drawing settings (`ground_rule`).

**Plan sets in the plugins; the plan's name in the title block (owner, 2026-10-24)**

- A plan set (`.ohlset`) lists the drawings to make in one go, each with
  its template (by name), paper and scales; plugins ship them beside
  their sheet templates (manifest `[[plan-set]]`), Export › Plans ›
  Plan set… writes one. The LeV plugin gives the ESTI 235 plan
  submission (sheets A1 1:2000/1:200, route plan 1:10 000, situation
  plans A1 1:1000, phase plan).
- The computed field `{plan_kind}` names the drawing in the title block
  (legend labels `plan_sheets`, `plan_route`, `plan_situation`,
  `plan_phase`, worded by the template); the LeV template uses it.

**The route, situation and phase plans (owner, 2026-10-24; roadmap O14)**

- A menu of their own, Export › Plans (owner: apart from the sheets),
  each plan written for every route with the Drawings window's template,
  title fields, routes and cases; `ohl drawing --plans` likewise.
- The route and situation plans are north up (not straightened along
  the line as the sheets' plan), the sheets placed along the route by
  what fits at the scale (the situation plans overlapping by a tenth),
  over the project's drawings within 1 km (full detail) and orthophotos;
  the axis, wires, structures numbered, corridor, north arrow, scale bar,
  legend.
- The phase plan compares the arrangement of the wires leaving each
  structure ahead (by level from the top and place from the left, with
  their circuit and phase — not by height, so a taller structure with
  the same arrangement is not drawn), the first and last always drawn;
  circuits by number with a key (long names made the figures
  unreadable).
- Plan sets (several at once) come next; links to public map services
  later (a separate step).

**Unequal wind: the exact closest approach (owner, 2026-10-24)**

- The closest approach of a pair is the exact shortest distance between
  the two wires (segment against segment), not at stations; the station
  spacing is not a setting of this check any more. The levels stay
  solved once for every wire; per pair the combinations are taken
  nearest first by the wires' boxes and stop once no box can come
  closer. The wires drawn for a result are those measured (a bundle's
  subconductor), so the line of the distance ends on them. The window
  says the requirement is C₃ / C₄.

**The stringing table smaller (owner, 2026-10-24)**

- Without a `{st.phase}` column the table has one row per tension
  section and circuit, its phases' tensions in one cell (as the band row
  writes them); the plan-submission template drops its Phase and "bei"
  columns (the case in the heading) and its rows are smaller. A bottom
  row that runs into the title block is now said as a warning (it
  overlapped on smaller papers).

**String checks (owner, 2026-10-24)**

- Checks › String checks… and `ohl strings` (`ohl_clearance::strings`):
  every string as solved against limits, by rules (`[[string_rule]]` in
  a rule file); a design-code plugin may give the rules as a preset (its
  template of the values to respect, `checks = ["strings"]`).
- The maximum load is the string's own (owner: defined in the string),
  a plain comparison with the solved axial force (owner: no factor). The
  swing limits stay on the string too (owner) and a rule may give its
  own: each limit given is checked, a row each.
- Uplift is a suspension string **going slack** (owner: some suspension
  strings hang upside down by design, so not the geometry); one given a
  compression limit, pushed beyond it, is uplift too.
- A rule takes strings by kind and by the voltage of their sections
  (owner). The kind is the solve's: a dead end is a string flagged
  strain, a lone string the solve treats as one, or the string holding
  the clamp at a section's end (`StringState::strain`, new); the others
  are suspension strings (a TAK's suspension string among them).
- The menu's greyed "String swing (to come)" is replaced by the check.

**Weather cases: no duplicates (owner, 2026-10-24)**

- Criteria › Design code no longer puts in a case the project already
  has: its cases are matched to the project's as the clearance presets'
  are (by weather, then by name; `preset_cases::Plan`), the load cases
  naming the project's case; only the others are added, after the
  project's (the code's file holds both; the project's own file is left
  as it is). Before, the code's file replaced the model's cases.
- **Criteria › Merge duplicate weather cases…** (the roadmap item of
  2026-10-21, owner: go ahead): the cases of the same physics in groups,
  each merged into the case kept; one command (`Command::Batch`: the
  library files through `Session::library_puts`, checked as one put
  is, and the line's settings), undone as one. Renamed where a file
  names weather cases (keys `cases`, `case_pairs`, `weather`,
  `creep_case`, `weather_cases`, a `[cases]` table), each name once in
  a list; patterns left as written. Saved check reports are not
  rewritten: they are records of a run as it was made.
- Roadmap: U3 (sheet breaks at structures off the alignment) and U9
  (project lines as full copies) taken off — built (owner); what U3
  still held, background maps imported into the project, is O16. U11
  kept, said in the jumpers' theory (owner: unless a fix is fast).

**The distance between conductors under unequal wind (owner, 2026-10-24)**

- A second check in the same plugin submenu as the distance between
  conductors in the span: under **unequal wind**, as codes ask (e.g.
  the Swiss LeV, Annex 6, 2.2, with ESTI's directive 240: one conductor
  at a wind p, the other at 100 … 60 % of p, p from none up to the full
  wind since the closest approach may come at less). The required
  distance is the plugin's (there C₃ within a system, C₄ between
  systems, from the same table; no sag or string term). OpenOHL
  measures (`ohl_clearance::unequal_wind`, check id `unequal-wind`).
- The core change was asked and approved (owner: yes). The user
  chooses the weather case (its wind, the most) and the wind step; the
  case's temperature and ice are kept, only the wind is stepped
  (owner); the wind from **both sides** (owner); the pairs as for the
  distance in the span — the same span, nearby spans when ticked, the
  circuits ticked (owner). The other conductor's least share is a
  setting (60 % by default).
- The line is solved at every step from none to the case's wind (the
  case's included) and at the share of each step, so the largest
  difference is always among the levels; between the levels the
  approach is not measured (a simplification: the step is the user's).
  Each combination is measured on stations every 10 m, the four closest
  again every Δs (a simplification: a combination's minimum is found to
  the stations' spacing); the distance is between the axes less the
  polylines' bounds (conservative).
- The same window as the distance in the span (its values tables, the
  admin's edits, saved reports, the circuits' insulation level), with
  the case, the step and the share instead of the cases and the k
  diagram; per pair the closest approach, the wind on each and the side.
- A result clicked shows only that span's pair as it was measured
  (owner): the two conductors at their own winds drawn on top, labelled,
  kept from the run (and in a saved report) — no case switched, nothing
  solved again. Closing a distance between conductors page shows again
  the cases shown before its first result switched them (owner), unless
  another case was chosen meanwhile.

**The sag on the profile sheets (owner, 2026-10-24)**

- The sag of each span can be shown on the profile sheets, per circuit
  (the earth wires apart), in the case each circuit is drawn in: as a
  band row (`sag`) and as a label under each circuit's lowest wire at
  mid-span (`[sag_labels]`), each switched in the template (owner:
  both, chosen in the template; each circuit). The plan-submission
  template shows both. The necessary ground clearance is the existing
  line under each circuit's lowest conductor, lowered by its rule's
  clearance (`[conductor_clearance]`).
- The necessary ground clearance as **one line for the whole line**
  (owner: not always the lowest conductor's limit governs): at each
  station the lowest of the circuits' lines (each circuit's lowest
  conductor lowered by its rule's distance), the crossings where another
  circuit governs added — the ground must stay under it
  (`[conductor_clearance] envelope`, on in the plan-submission template).
- The band row `ground_clearance` (Bodenabstand) gives per span the
  least distance from the ground up to that limit line (owner: agreed
  2026-10-24), in red below 0: the sheet shows the line is not hitting
  the ground.
- The stringing tension is given per phase of each circuit (owner): the
  band row lists the phases on the circuit's line, the stringing table
  has a row per phase (`{st.phase}`).
- One rule for what to solve again after an edit (`ohl_line::diff`):
  the project's dirty sections after a command and the differences of a
  project line shown with the line worked on now come from the same
  comparison (they had drifted, the cause of some of the kept results
  found stale).
- After an edit of the routes, the sections whose wind side changes
  (the line's direction where they are) are solved again; before, none
  was (found while answering the review's questions).

**Edits stay fast (owner, 2026-10-24)**

- The owner: visuals and edits must stay snappy, with the calculations'
  speed and accuracy the priority from the start. A span edit on a real
  line took 1.9 s (139 s with every weather case solved); now about 90 ms
  ([performance](performance.md), edit benchmark). Every result is the
  same as before: solutions kept and taken back only while what they are
  computed from is unchanged.
- After an edit the weather cases shown, drawn per section, picked or
  asked for by a view in the last 2 s are brought up to date at once; the
  others are no longer kept solved and are solved again from the line
  when next wanted (in the background for a view, before a check), like
  a case never shown.
- The menu of the loads sent to the structures keeps its height: its
  load cases and weather cases each scroll in a box of their own.
- What the windows and views ask for every frame (sheet templates, the
  profile's corridor, the thermal table's ratings, the routes) is kept
  while what it comes from is unchanged; the lines on the ground keep
  showing their other wind side's solution before while the new one is
  computed (display only, computed again as before).
- A code review of the kept solutions found inputs missing from their
  keys, now in them (each a case where a kept result could differ from a
  fresh one): the jumpers' cut lengths and kept solutions now depend on
  the line's defaults and solver settings too (a change of the jumpers'
  exit angle or stiffness factor kept the old cut), a jumper's kept
  solution on its cable's data; two versions of a line compared (project
  lines shown, the lines on the ground) differ in every section when a
  setting every solve reads differs, and in a spacer's sections when it
  does; the lines on the ground solve a project line with its own wire
  library and again when the libraries are read again; sections sharing
  an assembly or joined by spacers are coupled again from each solved
  alone, as a full solve does (the result no longer depends on the edits
  before); nothing is solved in the background while a structure is
  dragged. Tests compare a kept result with a fresh solve after such
  edits. Weather cases solved in the background: four at a time (each
  spreads over the cores; the next starts when one is in). A line with
  a number that is not finite (NaN, infinity) is refused when checked
  (`Line::validate`, every edit), saying where it is: stored as JSON it
  would no longer read.
- A jumper on a real line that does not solve in a narrow band of
  temperatures is left to the project's engineer (its design), not
  worked around in the solver (owner, 2026-10-24).

## 2026-10-23

**Key map and plan backgrounds (owner, 2026-10-23)**

- The key map's structure names are thinned so they stay readable
  (none touching another, the route, the sheet's box or the map's
  edge; the first and the last structure first; connections beside the
  line not named); the box around the sheet's stretch has its own pen
  (`box`, red in the plan-submission template).
- The project's drawings and orthophotos may be shown under the key map,
  north up, switched in the template and the export window.
- The backgrounds under the plan stop at the corridor's edges (the area
  shown) instead of filling the panel above and below it.
- Background maps imported into the project, per view, are on the
  roadmap (U3 follow-up 10).

**A stringing table on the sheets (owner, 2026-10-23)**

- The owner asked for the list of tension sections with their stringing
  tension on the sheet (ESTI 235, 2.2.2 a): a template table
  `[stringing_table]`, one row per tension section on the sheet and
  circuit (from structure, to structure, circuit, tension in kN, the
  stringing case), the earth wires last. In the bottom row beside the
  title block it keeps the profile's height (on a real line's A1 sheets
  the profile was 44 mm lower with it above the title block). The room
  is the same on every sheet; a sheet with more rows says so. The
  stringing band stays a choice of the template.

**Wind perpendicular to each span kept (owner, 2026-10-23)**

- Wind keeps acting horizontally and perpendicular to each span, at the
  full pressure, wherever the span runs. Compared on a real line with
  another program, short steep spans leaving the line at an angle gave
  results there that depend on the side the wind comes from (up to
  13 % in tension on the same span), here none: that program appears
  to blow perpendicular to the line, so part of its wind acts along
  such a span. Perpendicular to each span is the larger transverse
  load on every span (conservative); the owner keeps it.
- On those spans (about 1 cm of slack) the tensions here were also
  higher under ice, as if the other program's wire were 5–6 mm
  longer; an analytical catenary between the clamps solved here gave
  the same unstressed lengths to 0.1 mm, so the solver is unchanged.

**Vegetation: the swing arc kept (owner, 2026-10-23)**

- Measured on a real line against another program's report: wind
  stepped every 10 Pa matches it within about 2 cm (median), the swing
  arc without its band margin is 0.07 m more conservative, with the
  0.15 m margin 0.21 m; the steps take about five times as long. The
  owner keeps the swing arc with its margin (the more conservative) in
  the vegetation preset; the steps stay a choice in the window.

**Falling trees measured to the subconductors (owner, 2026-10-23)**

- Compared with another program's report on a real line, the
  falling-tree margins of the same trees were 0.2–0.3 m more severe
  here: the distance is measured to the outer surface of the
  subconductors of the bundle (radius of a bundle of three at 0.4 m:
  0.23 m), where the other program seems to measure to the bundle's
  centre line. The owner: measuring to the conductors in the bundle is
  right; no option to measure to the centre line, the core unchanged.

**Library files with every project line (owner, 2026-10-23)**

- A line made from a revision, a tag or a bundle gets that revision's
  library files too (it started without them before).
- Library changes are tracked per line like its objects (commands, the
  log, revisions, diffs; already so). A project line shown next to the
  one worked on is now read and solved with **its own** library files
  where they differ (wires; structure types and insulators: its
  structures resolved with them): its sections on a wire type whose data
  differ count as different and are solved with its data; the window
  says which of its library files differ.

**Project lines shown: shared solutions, only the differences drawn (owner, 2026-10-23)**

- Measured on a real line: starting a project line costs 0.5 s in the
  file; showing one solved it in every weather case of the project on
  the window's thread (105 cases, about 20 s release, the window held).
- Now a line shown is solved per weather case when that case is
  displayed, in the background: the line worked on's solution in that
  case copied and only the sections that differ solved again
  (`SolvedLine::update`; the same result as a full solve, tested). A
  section differs when it, or one of its structures (types resolved),
  differs; jumpers when they or their end sections do.
- Drawn: only what differs, by default (owner's choice), a tick for the
  whole lines. The lines on the ground, the clearance checks and the
  sheets take the whole line (a case not solved yet is solved for the
  sheets).
- Step 2 (owner: yes, after step 1): the project file keeps a new line
  as its starting revision and what it changed (roadmap U9).

**New project lines keep only their differences (owner, 2026-10-23; roadmap U9)**

- Starting a project line no longer writes every object of the line it
  starts from again: the line records the revision it starts from as
  its base (`vc_ref.base`; the revision recorded on the source line, or
  the revision or tag it is made from) and keeps in `model_object` only
  the objects it changes or adds since, and a tombstone (`gone`) for each
  object of the base it deletes. Reading it: the base revision's objects
  and library files with its rows laid over them. Project schema
  **6**; an older program refuses such a project (it would read the line
  as nearly empty).
- Older projects are read as they are: a line without a base (the main
  line, every line of schema 5 and before) keeps all its objects, and
  nothing is rewritten when they open. Lines started from them are kept
  as differences.
- Why a revision and not the source line: revisions are immutable and
  never deleted, so deleting or renaming the line a line came from
  cannot change it, and the base is already the two lines' common
  ancestor for bringing changes in.
- Not compacted: the rows of a line are bounded by the objects it ever
  changed; moving them onto a newer revision would gain little and
  reset the objects' versions.
- Visible differences: none in the lines, their library files, the log,
  undo, merges or bundles. The objects of a new line start again at
  version 1 (they kept the source line's numbers; versions are not shown
  and undo judges by content). A line's spatial index rows are written
  when it is first worked on, not when it is started. Lines made from a
  revision or a bundle start with that revision's library files (the
  owner's answer, below).
- In memory the lines share the JSON of the objects they hold alike.
  Measured on an invented line of 1 101 structures and 552 sections
  (2 770 objects, five library files of 200 kB): starting a line 67 ms →
  34 ms, rows written 2 770 objects and 2 756 index rows → none, the
  file +2.88 MB → +0 MB, memory held +18.3 MB → +3.1 MB (the second
  line's typed copy); the project with two lines opened again holds
  37.6 MB → 22.5 MB and opens in 62 ms → about 77 ms (the base revision is
  read from its compressed blobs); working on the new line for the first
  time writes its index (about 4 ms).

**Sheets: cases per circuit, the stringing band, strings drawn (owner, 2026-10-23)**

- Each circuit's wires are drawn in a weather case of their own: by
  default the phase conductors at 80 °C (the largest sag) and the earth
  wires at 10 °C (owner's answer), chosen per circuit in the export
  window over the template's `[cases]` (a case's name or a
  temperature).
- The stringing tension (Montagezugspannung) is per tension section —
  between two dead-ends, not per circuit (owner's correction) — in kN:
  a `stringing` band row writes each circuit's and the earth wires'
  horizontal tension (per subconductor, the lowest and highest of the
  section's spans) in the stringing case (10 °C by default).
- The strings are drawn on the structures in the profile and in the
  elevations (all of them, also with the full structure).
- The key map's structure names can be left out (`[key_map] structures
  = false`; the ESTI template does). A background map under the key map,
  chosen in the template, is for later (roadmap).
- The ESTI template: role colours (project red, existing black, removal
  yellow, other lines blue; the voltage kinds without colour words), the
  cases above, the stringing band instead of a typed field.

**A TIN layer per clearance rule (owner, 2026-10-23)**

- A rule of the Vertical and Radial clearances windows checks a terrain
  surface, LiDAR classes, or both; the surface is now chosen per rule:
  none, the surface of the checks (as before), or any TIN layer of the
  project by name. In the rule file a ground rule's `surface = "name"`.
- The named surfaces are read from the project when a check starts (the
  one shown and the checks' surface shared), in the windows, the
  Clearances page, `ohl zones` and `ohl line FILE.ohl --rules`; a name
  the project does not hold is noted and the rule not checked (never
  passed silently).
- Also in the thermal limit (2026-10-23; built, in active development,
  not approved yet): Analysis › Thermal limit and `ohl thermal
  clearance-limit FILE.ohl` read the surfaces the ground rules name. A
  name the project does not hold stops the search with an error rather
  than leaving the rule out, as the limit found without it could be too
  high.

**Drawings and orthophotos under the sheets' plan (owner request,
2026-10-23; built, in active development, not approved yet)**

- The project's DXF drawings and orthophotos can lie under the plan
  panel of the sheets. The choice is the sheet template's
  (`[plan_background] drawings`, `orthophoto`: `false`, `true` or a list
  of names), so a template (a plugin's preset) can switch them on; the
  export window's ticks and `ohl drawing --plan-drawings
  --plan-orthophoto --no-plan-backgrounds` switch them over it. Off by
  default: sheets without them are byte for byte as before.
- **Straightened like everything else in the plan**: a point at station
  *s* and offset *o* (projected onto the centreline as the wires and
  structures are) is drawn at *s* along the sheet, *o* across it. Each
  segment of a drawing is cut into pieces of at most 2 m first, so a long
  straight line follows a bend on paper. At bends the outside wedge is
  not shown and the inside one twice, as for the wires.
- **Drawings**: taken within three times the line width (or the
  corridor) of the axis, clipped to each sheet's plan panel, on the layer
  `OHL_BACKGROUND_DRAWINGS`, under everything else; by default one grey
  pen (0.13 mm `#8c8c8c`: the line stays readable), `own_colours = true`
  keeps the DXF's colours (white drawn black). Texts and points off by
  default (texts at their size on the ground, not turned: the drawing
  model has no turned texts). Not in the plan's model-space DXF: the
  drawing itself is already in project coordinates.
- **Orthophotos**: resampled per sheet, pixel by pixel (the exact
  inverse of the straightening, per column of pixels the centreline's
  segment and normal), at `dpi` (default 200: an orthophoto of 0.25 m
  at 1:2000 is about 200 dpi on paper), from the coarsest pyramid level
  not coarser than a pixel, bilinear; a 60 % white veil by default;
  written as one JPEG (quality 75) per sheet: embedded in SVG, the JPEG
  itself in PDF, drawn in PNG. **DXF R12 has no images: the photo is
  left out of the DXF sheets with a warning** (no external image
  reference written: the DXF stays one self-contained file). At most 40
  million pixels per sheet; the sheet preview draws it at 60 dpi.
- Display only: no computed value and no sheet break depends on them.

## 2026-10-22

**Elevations as stick figures with their heights (owner, 2026-10-22)**

- A tickable **stick figure** for the elevations, even where the full
  structure exists (the lattice is unreadable at a sheet's scale): a
  schematic one — mast, an arm out to each point, the strings — always,
  since an imported line's stick drawing may hold the whole lattice.
- Strings: only the **vertical (suspension)** strings' lengths are
  written, per arm; no string labels otherwise.
- Heights: base → first arm → between the arms → top as a chain on one
  side; each arm's height **above the base** (owner's answer) on the
  other, one line per arm with its strings' lengths.
- Never over the structure or each other: columns clear of the figure,
  texts pushed apart with a leader.
- The ESTI 235 template of the private plugin ticks them all.

**Mixed paper sizes stay free (owner, 2026-10-22):** no rule keeping one
sheet height — the goal is that the drawing fits.

**Lag on a real line: picking and the profile (2026-10-22)**

- Measured on a copy of a real line (lattice towers, two DXF drawings):
  a click took 3.1 s (the rigid links' joints looked up quadratically
  for every structure, though none had a link), and the profile under
  the 3D view drew every structure's every member each frame, off the
  screen too (175 000 shapes, 21 ms a frame).
- Fixed without changing any computed number: no joints placed for a
  structure without an offset link, joints listed once; the profile
  draws only the structures in view, and a structure under 80 px tall
  with more than 40 bars as its mast and arms (display only), its dots
  as one mesh. A click 25 ms, a frame about 2.6 ms.

**DXF drawings in the 3D view: level of detail (2026-10-22)**

- Asked whether a DXF background's level of detail is defined: it is.
  Drawings are a chunked layer of the Level of detail table (indexed
  once, built per 256 m chunk and level on the GPU, culled by frustum
  and render distance, simplified by Douglas–Peucker at levels 1 and 2,
  texts in the plan only); measured with an invented drawing of up to
  16.8 million vertices (`bench::drawing_benchmark`), the 3D view's CPU
  share of a frame stayed under 2 ms and the frame under 10 ms.
- The Drawings row's defaults are now level 1 from 300 m at 0.2 m and
  level 2 from 2 km at 1.5 m (were 1 km at 0.5 m and 4 km at 2 m): about
  a pixel's error where each level begins; a fifth to a third fewer
  segments drawn. Rows saved with the former defaults read as the new
  ones (`LodRow::former_default`); values of the user's own are kept.
- An edit of the line (`refresh_views`) and showing or hiding the whole
  Drawings layer no longer index the drawings again (0.4 s for 16.8
  million vertices, every built chunk dropped); hiding or showing one
  drawing still does.
- Display only: no computed number changes.

**Sheets: paper size and orientation optimised (owner, 2026-10-22)**

- A sheet choice **optimised** picks size and orientation per route from
  the sizes ticked (a list of standard sizes: ISO A0–A4 by default, the
  long A sizes, ISO B, ANSI, ARCH, checked against a published list).
  Ranking: fewest warnings, then least paper (fuller sheets), then fewest
  sheets, then the smallest sheet. The scales are never changed.
- **Different sizes** on one route (a tick): only sizes of one standard,
  one orientation; breaks and sizes chosen together (dynamic programming
  over the places a sheet may end, the profile's fit estimated from
  heights joined stretch by stretch, the chosen sheets then laid out in
  full and ranked with the others).
- **Same paper on every route** (a tick): one size, or one standard
  when mixed, best over the routes together.
- Templates may ask for it (`[page] format = "optimised"`, `sizes`,
  `mixed`); the command line has `--page optimised`, `--paper-sizes`,
  `--mixed-sizes`, `--same-paper`.
- The sheet preview draws only the sheet shown, in the background, and
  again only once the settings have settled for 0.5 s; its window
  chooses the route and steps through the sheets; a number past the last
  shows the last.

- **A rule preset may hold only wire tension rules** (2026-10-22): the
  host accepts a plugin preset with `[[tension_rule]]` tables and no
  `[[rule]]` (as for `[vegetation]`), so a design-code plugin can offer
  presets of the wire tension check alone.

- **A rule-based wire tension check** (owner, 2026-10-22: the maximum
  allowed load on wires). Built, in active development (not approved
  yet).
  - Checks › **Wire tensions…** (Run footer, explanations, plugin
    presets) and `ohl tensions` (CSV with `--csv`; exit status 1 when a
    span fails). Rules: targets (wire types, systems, circuits, or
    all), weather cases, a **factor of the RTS** (allowed = factor ×
    RTS), and per rule the tension compared: the **maximum** at the
    span's most tensioned support (default) or the **horizontal**.
  - Stored in the rule file format as optional `[[tension_rule]]`
    tables (additive; old files read unchanged); a preset may carry
    them, with `checks = ["tension"]`, and the window takes only those.
  - Results: every span and wire per case — tension, allowed, usage,
    pass or fail — sorted by usage; the failures (else the worst)
    marked in the views at the most tensioned support while the window
    is open.
  - **No factor of any code or utility in the public code or
    examples**: the example (`examples/synthetic-line/tension-rules.ohlr`)
    uses invented factors, labelled. The greyed "Tension limits (to
    come)" menu entry is replaced by it.

- **Always warn above the wire's strength** (owner, 2026-10-22). Any
  solved state where a span's tension at its most tensioned support
  exceeds the wire type's RTS gives a warning: in the line's solvers
  (ruling span and finite elements, `SectionShape::flags`: the Model
  panel's warnings, the Sections table's "above RTS", `ohl line`) and in
  `ohl sagtension` (each weather case; initial and both final states on
  the non-linear model). Independent of the range warnings. Built, in
  active development (not approved yet).

- **Sag-tension follow-ups** (owner, 2026-10-22). Built, in active
  development (not approved yet).
  - **Range warnings only where the cable gives a range**: without a
    component's `max_strain` there is no warning (the 1 % default for
    the warnings is gone; the curves must still rise up to 1 % to be
    inverted). `nonlinear-003…004` unchanged (the synthetic core's
    strains were within 1 %); `nonlinear-005…006` added (an invented soft
    cable beyond 1 %: no warning without a range, six with 1 % given).
  - **The sheave model is not wanted**: removed from the follow-ups; the
    in-the-blocks simplification stays, marked as such. *(Reversed by the
    owner on 2026-10-25: sections in the blocks over frictionless sheaves,
    the equal-H model an option.)*
  - **No Drake-like curves** ("Don't take it into the examples and checks
    if we don't have it."): the cross-check on curves from a library's
    test data of unstated source is removed — `xdrake-001…009`
    (`crosscheck-drake.toml`), their reference script
    (`reference_crosscheck.py`) and the criteria cases on that cable.
    The features they validated keep their validation on an **invented**
    cable (the synthetic curves with a core steeper than its final
    modulus, so a negative set) against a new independent
    re-implementation (`reference_invented_states.py`): `nlstate-001…005`
    (both final states, the governing one per case, after load equal to
    initial at the load-stretch case) and `nlcrit-001…005` rebuilt
    (tension criteria; invented limits). The code agrees with it to the
    case tolerance (1e-7). The validation page's Table 3 lead keeps one
    neutral sentence without numbers. `examples/sagtension/drake-section.toml`
    and the sagging cases stay: they use linear, published-type data only.

- **Lines on the ground: the weather cases and project lines displayed**
  (owner, 2026-10-22: "Only the displayed weather cases and project
  lines."). Built, in active development (not approved yet).
  - The lines take the weather cases the views display (View › Display
    weather cases; a section drawn in its own display cases, in those),
    both wind sides, instead of a case set of their own: the menu's case
    ticks are gone (it names the cases displayed); old settings with
    `shown_case` / `cases` are read as displayed.
  - The project lines displayed (the "Other project lines" layer shown)
    count, each as drawn (same display cases, both wind sides, their
    other wind side solved in the background as the working line's);
    a hidden one does not.
  - The envelope option is now "one envelope for all routes and project
    lines": a project line's span between the same two structure places
    as one of the line's is one span with both their wires; "each its
    own" gives every project line its own lines (not in the profile).

- **Vertical clearances: only straight below, drawn vertically**
  (owner, 2026-10-22: "When working with the vertical distance, it
  should only check the points vertically under the line within a given
  width in the parameters. [...] When drawing the results that are too
  short, it should also only be vertical, even at the 1 m mark.").
  Built, in active development (not approved yet).
  - **Checked against the code, unchanged:** a vertical rule takes only
    the points (and the ground) whose plan position lies within ± the
    width of the wire (`vertical_band`); the distance is the wire's
    height over the target at that plan position, interpolated along the
    piece (with a depth: the wire's lowest over the slice); targets
    beyond the width are left out entirely, no sloped or 3D distance.
    With the swing band the width is taken across the swung wire's plan
    position, the distance vertical. No number changes.
  - **Changed: the drawing.** Every vertical violation and the lowest
    reported is drawn as a vertical segment from the target straight up
    (down) to the wire's height at its plan position (`Hit::drawn`,
    `ZoneFinding::drawn`): the 3D view, the cross-section panel, the
    plan (the mark at the target's plan position), the profile (which
    now draws the clearance marks too) and `ohl line`'s viewer markers.
    Before, the segment ran to the wire point found, up to the width
    beside the target. Radial rules unchanged. The CSV reports keep the
    wire point found.

- **The cross-section view is a 3D panel** (owner request of
  2026-10-22: "For the cut with C, I would like it to be a 3D object
  still, that I can move around in. It just has to show in 2D at first.
  It should be a separate panel that contains only the data of the
  cut, but everything should work the same as in 3D."). It **replaces
  the 2D-only design of 2026-10-21** (zoom and pan, no rotation, drawn
  with its own painter). Built, in active development (not approved
  yet).
  - **Unchanged:** how a cut is opened (C, the window, View menu, a
    structure's menu, the profile's right-click), the plane's position
    and orientation, depth ±10 m and width by default, ◀ ▶ ⏮ ⏭ , . to
    move it, several cuts each in its own tab, the hover readouts
    (offset, height, height above ground, distance to the nearest
    conductor; now where the pointer's ray meets the plane).
  - **The panel** is the 3D view of the line — the same renderer,
    layers, weather cases, picking on every drawn copy, tools, drag
    handles and control schemes — with a **clip box**: only what lies
    within ± the depth and ± the width is drawn (the renderer discards
    every fragment outside the box, so surfaces, wires and points are
    cut exactly at its sides), culled (only the chunks and point nodes
    meeting the box are built: a filtered chunk set, no data copied)
    and picked. The indexes of the large layers are shared with the 3D
    view (made once per change, not per panel).
  - **Front view first:** orthographic (new in the renderer: the same
    view height as the perspective view at the target, so zooming
    works the same), looking ahead along the line, perpendicular to the
    plane, level, 1:1, the whole width in view; "Back to the cut
    view" and **Home** return to it; Orthographic / Perspective in the
    panel's bar. Small axes, a scale bar and the station label are
    drawn over it.
  - **Moving the cut carries the camera** (its pose relative to the
    plane; its height follows the ground at the plane, a choice made
    here so stepping along a slope keeps the ground in view).
  - **Exaggeration dropped** in the panel (the owner: not needed in
    3D): heights are always 1:1. (The 2D view offered ×2–20.)
  - **The main 3D view outlines every cut's slab** (light blue, its
    name at a top corner); a click on an outline where nothing else is
    picked brings the cut's panel to the front. Dragging the plane in
    the 3D view (a gizmo) is not built.
  - **Not yet** (follow-ups): export of the front view (SVG / PNG /
    DXF), the swing band and the clearance circle, the gizmo.
- **The non-linear model after the evaluation of a public-domain
  sag-tension library** (owner go-ahead of 2026-10-22 on its
  recommendations). Built, in active development (not approved yet).
  The library is used as a reference only; none of its code is copied.
  - **Independent cross-check cases** (`crosscheck-drake.toml`,
    `xdrake-001…009`): the shell/core model on a Drake-like cable whose
    curves come from that library's test data, **source not stated**
    (test input only, not CIGRE TB 324 data); expected values from an
    independent re-implementation, the library's own run as a second
    reference with the conventions matched (its own weight per
    stretched metre emulated; its first-order length update, 101-point
    mean and absolute tolerances explained): −0.004 to +0.022 %.
  - **The permanent set is signed.** Each component's set
    \\( e - P/EA_f \\) was clamped at zero; a component whose initial
    curve is steeper than its final modulus (the Drake-like core) gets a
    slightly negative one, so after load the final tension equals the
    initial at the load-stretch case (−0.14 % before; checked to
    \\( 10^{-6} \\), `xdrake-007`). Of the two equivalent forms (a signed
    strain, or the stretch stored as a load as the library does) the
    signed strain was kept: the final state, the FE model's stretch and
    the reports carry a strain per component already. The final state is
    \\( \min \\) (final-modulus line, initial state), continuous at zero
    strain. No earlier case changed (the synthetic cable's sets are
    positive).
  - **A slack cable's strain at zero tension** (found by these cases):
    when every component takes no compression, it is now where the
    cable starts to carry tension, not the search bracket's lower end
    (−0.1 %, which made such a cable's own weight per metre 0.1 % too
    low). The Drake-like initial tensions moved by −0.03 to +0.04 %; no
    earlier case changed beyond \\( 10^{-12} \\). A trial tension so slack
    (the catenary parameter below half a span) that the stretch does not
    settle now counts as too slack in the non-linear bracket instead of
    stopping the solve.
  - **Both final states; the governing one per weather case.** After
    creep and after load are both solved and reported (validation
    outputs, `ohl sagtension`); in each case the one leaving the wire
    longer (the lower H, the larger sags) governs — before, the state
    longer at the creep tension was used in every case (always "load" in
    the cases run). With the Drake-like curves, after load governs up to
    120 °F and after creep from 167 °F, and at the load-stretch case
    itself (`xdrake-001…009`). A tie (\\( 10^{-9} \\)) goes to the load.
    The line's Max Sag calculation already took the larger sag per case.
  - **Tension criteria on the non-linear model**, each in a condition
    (initial, after creep, after load): a wire with curves takes its
    `ohl sagtension` criteria on its curves (`condition`, default
    initial); each trial stringing tension recomputes both permanent
    sets; the governing criterion is found by iteration (a bisection per
    criterion, then all checked at the smallest, again while one is
    above its limit; 10 passes at most), not in a single pass. A linear
    wire refuses a condition after creep or load (no permanent set).
    `nlcrit-001…005`.
  - **Sagging outputs** (post-processing, no engine change):
    stopwatch (the travel time along the catenary with its local
    tension), dynamometer, transit angle of sight (the tangent from the
    instrument; the target on the far support), the level-span sag and
    the inclined-span correction, the sag per kelvin, and clipping
    offsets when a section sagged in the blocks is clipped in at another
    temperature (zero at the same temperature in the engine's model).
    Each formula derived in the theory; `ohl sagtension` `[sagging]`; the
    section window's **Sagging** panel; `sag-001…003`.
  - **Not changed**: the in-the-blocks model (one horizontal tension
    through the section). Equal support tension across sheaves, and
    sheave friction, would be an engine change the owner has not
    approved: **listed as a follow-up** (roadmap Part A).
  - **Beyond the curves' range**: a state whose strain (at a span's most
    tensioned support) passes a component's `max_strain` (optional, 1 %
    when absent; files unchanged) is warned about — the validation
    output `beyond_range`, `ohl sagtension`, the app's warnings by
    weather case (ruling span and finite elements) and the Sagging
    panel. The polynomial is still evaluated (no extrapolation is
    invented). `nonlinear-003…004`.
  - **CIGRE TB 324 Table 3** stays a note in the validation page (the
    test curves reproduce it to 0.05–0.06 %), not a case, until the
    owner confirms the curves from an admissible source.
- **Lines on the ground beside the line round every span** (owner
  request: "The lines on the ground on the side seem to not go to the
  substations properly and do not show for lines outside the main
  alignment"). Built, in active development (not approved yet). What
  was missing: the lines were made per alignment, between structures
  following each other on it, so the spans to a structure away from the
  alignment (a substation gantry, a portal beside the line) had none,
  and the lines of a route and of its branch were separate lines,
  ending at the tap. Now:
  - **Every span** of the line, as a graph (`ground_lines::Graph`):
    every span of every section between two structures (once, however
    many sections and weather cases hold it), on every route and
    branch, the spans to structures away from the alignments included,
    each in its **own direction**; from the axis, every leg of every
    alignment. The jumpers count at the structures they end at; a
    jumper on a tap in a span, with that span.
  - **Joined at the structures**, the sides followed from span to span
    round each structure (the next span clockwise), the jumpers
    included as before: round outer corners in 5° steps; at inner
    corners and taps the lines cut each other off where they meet — no
    gap at the tap.
  - **Closed round the ends**, decided here: a **round end**, not a
    square one — at a structure with one span (the last tower, a
    portal, a substation gantry) the line goes right up to it and round
    it, the distance beyond the ends of the wires and jumpers there (the
    outline of their ends' convex hull widened by the distance: round
    about a single wire's end, straight across between several). The
    owner wanted continuity, hence a closed outline; a square end at the
    structure would leave the distance round the wires' ends unkept.
    Where a span's first or last stretch has no wire within the
    distance of its chord, the end's outline joins the first point that
    has.
  - **Routes side by side or crossing**: a new choice in the menu, **One
    envelope for all routes** (the default: whatever lies within the
    distance of another route's wires is cut off, so each side line is
    the outer envelope of the routes ticked) or **Each route its own
    lines**. The route ticks list every route and branch of the line
    (and any route only its structures give), always. A span is drawn
    when it is on a route ticked or on none. View setting
    `ground_lines.route_lines` (`envelope` | `each`; older settings read
    as `envelope`).
  - **Tolerances** (display only, no computed result changes): a point
    is cut off when it lies nearer than the distance less 0.2 % of it
    to a wire of another span (so the line's own crossings, a few
    millimetres inside at worst, stay); pieces are joined where their
    end and the next start lie within half the distance (at least 5 cm,
    at most 1 m), at the crossing of their last and first segments.
  - **The profile** takes the points beside each alignment by the side
    of the span they lie beside (tagged), not by their projection
    alone; the points round an end or beside a span away from the
    alignments are in no profile.
  - Unchanged: the LiDAR corridor, the set of weather cases (both wind
    sides), the draping and the gaps; the other project lines are still
    not taken into account. Tests: the branch (S3) gets its lines on
    both sides; a span to a gantry beside the branch's end gets them
    and the line closes round the gantry; no gap at the tap T3; the
    envelope option (synthetic and on the example);
    `ground_lines_tests.rs`, `ground_lines.rs`.

- **Orthophotos: JPEG-compressed YCbCr GeoTIFFs read** (owner report:
  importing GeoTIFFs written by GDAL with `COMPRESS=JPEG
  PHOTOMETRIC=YCBCR` failed with "Unhandled TIFF color type YCbCr(8)").
  Built, in active development (not approved yet). The `tiff` crate
  (0.11, already a dependency, its default `jpeg` feature) decodes
  JPEG strips and tiles with their `JPEGTables`, but gives the samples
  in YCbCr, which the `image` crate's TIFF reader refuses; a TIFF the
  `image` crate does not read is now read with the `tiff` crate
  directly (`ohl_background::tiff_image`) and turned into RGB as JPEG
  (JFIF, BT.601 full range) defines it. **No new dependency**, no
  external converter needed. Grey, RGB and RGBA of 8 or 16 bits are
  read that way too. Any other variant (a palette, CMYK, Lab, floating
  point, more bands…) is refused with a message naming it — colour,
  bits, sample format, compression, strips or tiles — and the
  workaround (convert to JPEG with a world file, or to a plain RGB
  TIFF with `gdal_translate`). Tested with small TIFFs written in the
  test as GDAL writes them (abbreviated JPEG streams, tables in
  `JPEGTables`, no JFIF marker), tiled and in strips, with GeoTIFF tags
  and with a world file only, and a plain RGB deflate TIFF: their
  places and the colours of each block (within 6 levels after JPEG).

## 2026-10-21

- **A cross-section view** (owner request of 2026-10-21). Built, in
  active development (not approved yet). **C** (or View ›
  Cross-section…, a structure's menu, the profile's right-click with
  Select held) opens a window: a station on an alignment or a structure
  (a searchable list, or clicked in a view), the depth (**±10 m** by
  default) and the width across (the line width by default); **Open
  cut** opens a 2D view as a tab of its own (several at once).
  - **Orientation.** At a station the plane is perpendicular to the
    alignment; within 1 cm of an alignment point, along the bisector of
    the two legs (as structures stand). At a structure, the plane holds
    its local x axis (its cross-arms); turned to look ahead when the
    structure is turned round. The view always looks ahead (increasing
    stations): the left of the line on the left; said on the view.
  - **What it shows** (within ± the depth along, ± the width across,
    projected): the ground (the active terrain surface cut exactly by
    the plane, else the lowest LiDAR ground point every 0.5 m), the
    LiDAR points (class visibility honoured), the wires of the cases
    shown (both wind sides: the blowout; a marker and label where each
    crosses the plane, the curve within the slab), jumpers, structures
    (members or stick figure, strings as solved, attachment points,
    joints), the corridor's edges, the crossings of the lines on the
    ground. Same scale both ways by default (exaggeration 2–20
    optional). Hover: offset, height, height above ground, distance to
    the nearest conductor crossing. Clicks select as in the other views.
  - **Navigation**: zoom, pan, fit; no rotation. ◀ ▶ (and , .) step the
    cut by a step (twice the depth by default), ⏮ ⏭ to the structure
    before or after. C was free (G, M, E, 1–9, W A S D, Space are
    taken; ⌘C is left alone); , and . were free.
  - **Performance**: the LiDAR points of a slab come from a grid index
    of the points loaded (8 m cells, made once, again when points
    move); draw-only thinning (one dot per pixel and colour). Wires and
    structures are taken every frame, so the cut follows edits and
    re-solves.
  - **Not yet** (follow-ups): the swing band and the clearance circle
    around the conductors (optional in the request, left off); the
    export of a cut (SVG / PNG / DXF through the drawing model); points
    streamed only for the 3D view are not in the cut.
- **Check windows: Run at the bottom left; a rule's voltage filter
  optional; marks off when a window closes** (owner requests of
  2026-10-21). Built, in active development (not approved yet).
  - **Run footer.** Every check window and page — Vertical and Radial
    clearances, the vegetation panel, Safety volumes, Galloping
    ellipses, As-built wires, Distance between conductors, Thermal
    rating, Structure loads, Bill of materials, Spot structures and the
    Clearances tab — has **Run** at its bottom left in a footer that
    stays visible: the body scrolls above it (`check_help::body`,
    `check_help::footer`, `check_help::run_button`, one helper for all).
    Stop (where a run can be stopped) and the progress are in the
    footer; the out-of-date banner stays at the top with **Run again**,
    greyed when Run is. The buttons named Check, Fit, Spot, Compute,
    Compute again, Check now and Clearance-limited temperature are all
    **Run** now (their hover says what they run); Cancel is **Stop**.
    The vegetation panel's and the clearance windows' Run had moved to
    the top because a click on a widget scrolled out of sight did not
    register in the tests; the footer is always visible, so it is
    clickable, and the tests check it in every window with a short
    screen and the body scrolled.
  - **Marks off when the window closes.** The violations of a Vertical
    or Radial clearances window, and of the Clearances tab, are drawn
    while the window or tab is open (docked or moved: they stay),
    taken off the views when it is closed, back when it is opened again
    — as the vegetation panel's layers and the safety volumes already
    were. A window that never ran leaves the marks drawn by another
    alone. Galloping, as-built wires, thermal, bill of materials and
    spotting draw nothing in the views; the distance between conductors'
    line already went with its page.
  - **A rule's voltage filter is optional.** With one distance it is a
    tick menu ("only for", none ticked: every voltage); with a table by
    voltage or base + per kV, which already depend on the voltage, it
    hides behind **Only for some voltages…**, off by default. Rules in
    existing files with a table *and* a filter read and are checked as
    before (the filter applies); the toggle is shown on so the user can
    clear it. The rule editor now shows the filter (it kept it unseen
    before).
  - **No rule per voltage.** `ohl_clearance::zone::ZoneRule::voltage`
    (one voltage per row) became `voltages` (a list): a window imports a
    rule listing several voltages as one row instead of one row per
    voltage; the check and the rule file written are the same (the
    format is unchanged). `ohl zones`' summary reads the note of a row of
    several voltages ("no wire of 110 or 220 kV"). No other code path of
    the application made one rule per voltage; a design-code plugin's
    presets are the plugin's own.
  - **Merge rules by voltage…** (rule editor, offered only when there is
    something to merge): rules alike in every field but the name, each
    for one voltage with plain distances, become one rule with a table
    by voltage and no filter in the first one's place, after a preview
    of what changes at each of the line's voltages; one undoable
    command. Without the filter the merged rule checks every wire: a
    voltage between two rows takes the next row up (the larger distance
    when distances grow with the voltage), earthed wires the first row,
    and above the last row there is no distance (the check stops and
    says so) — the preview says so before anything is written.
  - **Remove all rules** (commit of the same day) checked: beside Add
    rule and Load preset…, asks first, Undo brings the rules back; a
    test clicks it, confirms and undoes.

- **Attachment points: the link follows edits, only attach joints take
  a link, `derive` writes joints, no point on a member** (owner
  decisions of 2026-10-21, after the Link tool). Built, in active
  development (not approved yet).
  - **The link follows edits.** Linking keeps the point where it is
    (the offset stored), as before. When a linked point is later moved —
    a drag (OHL-Design's 3D view handles and structure screen,
    OHL-Structure's view), the Attachment points tables (both
    applications, now typed for a point on a joint too), the Edit tool's
    structure and string windows, `ohl project apply` putting a
    structure — it stays on its joint and its offset is recomputed
    (offset = new position − joint, in the structure's axes), in the
    same command (one undo). In a structure file a point on a joint
    moved no longer moves the joint (`ohl_structure_edit::links::move_point`;
    before, a point on a joint dragged moved the joint); a joint moved
    keeps the offsets, so its points follow. In the line: the session
    applies `ohl_line::links::follow_moves` to a structure put by a
    user (an own linked point whose stored position moved in the
    structure's axes while its joint and offset are unchanged); the
    structure window carries its points' links (they were dropped
    before) and the string window keeps a moved point on its joint
    (`ohl_line::links::moved`). Not followed when the type, extension or
    height adjustment change in the same put (the joints move then).
    Properties has no position field for a point. Tests per path.
  - **Only attach joints take a link**, in both applications and on
    the structure screen: only they are ringed and clickable in the Link
    tool; another joint under the pointer says "not an attach joint:
    set its kind to attach in OHL-Structure" (`ohl_structures::NOT_ATTACH`).
    The same rule in the file edit (`link_point`), the line
    (`ohl_line::links::linked`, so `LinkPoint` from the applications and
    `ohl structures link`), a drag released on a joint, the Attachment
    points tables' joint list, and the exact match of a structure change
    (`match_points`: an attach joint at the point's position; before,
    any joint but a support joint). A joint carrying a point is an
    attach joint (worked out), so every existing link stays valid.
  - **`ohl structures derive`** writes version 6 structure types: per
    attachment a `[[node]]` `A<set>-<phase>` of kind `attach`, the
    attachment point on it, its wire point (set) and its string (the
    assembly, or inline length and weight; a strain string level, ahead
    or back); a wire point without a string fixed where it is. The old
    `[[attachment]]` form is still read. Tests: the derived types place
    back the source's points and string lengths; every point on its own
    attach joint.
  - **No point on a member.** A point needs a joint: `member` + `at` is
    no longer a way to define a point. A file written before that has it
    is read with a joint of kind `attach` inserted on the member there
    (id: the point's label, as before), the member split into two pieces
    that stay **continuous** (the same fields both sides: a rigid
    connection, moment continuity for frames; the first keeps the id,
    the second `<id>-2`), each with the whole member's buckling length
    as `lk` (`ohl_structures::on_members`). OHL-Structure and the
    structure screen write the file so when it is next saved.
    OHL-Structure gains **Insert a joint** on a member (the Members
    table's menu: the same split at a fraction, an attach joint; there
    was no such feature before). The FE "share to the member ends" path
    for a point along a truss member, the analysis-only split
    (`Model::split`, `Solution::pieces`, `MemberPoint`) are removed. A
    joint carrying a point where only bars meet in a line or a plane,
    loaded across them, is refused with the fix ("make the members
    frames, or add a member to the joint", `Model::bar_joint_error`)
    rather than held by the stabilisation (which would share the load
    to the bars' ends again). **Conservative:** each piece keeps the
    whole member's buckling length (a joint carrying strings braces
    nothing; set `lk` where it does). Migration: frame members give the
    numbers of before bit for bit (the analysis split them the same
    way; unit test, validation `structfe-014` unchanged, every-profile
    compared with the program before); the pieces are now reported as
    members of their own. A truss member with a point along it, loaded
    across, is now an error where its load was shared to the ends (no
    shipped file has one).

- **Roadmap split by application** (owner request of 2026-10-21, "split
  the roadmap between OHL-Design and OHL-Structure"). ROADMAP.md stays
  one file (the book and many pages link to it) with three parts:
  **Part A — OHL-Design** (the line), **Part B — OHL-Structure** (the
  structures, marked as an earlier stage, not for use yet) and **Part C
  — Shared** (the engine, formats, plugins, server and version control,
  validation and docs, Phase 6, further ahead). Each part has where it
  stands, open points, known limitations, proposed stages and the
  detailed record. A pure reorganisation: every item moved verbatim with
  its id (O1…, U1…) and its owner lines; an item concerning both
  applications sits where its work is, with a one-line pointer in the
  other part. Placed: A — O1, O3, O13, U3, U7, U8; B — O4, O5, O6, O7,
  O8, O10, O12, U1, U2, U4, U5, U6; C — O2, O9, O11. A one-line "next
  up" heads Parts A and B, taken from existing items and owner
  decisions.
- **Circuits: objects of the line, defined by the user, following the
  physical connections** (owner decisions of 2026-10-21: "circuit"
  stays the term for the name group, the conductors that form one
  circuit, and is to be definable fully by the user; then, correcting:
  "circuits can only be merged by physically connecting them; two
  circuits that are not connected must never be merged"). Built, in
  active development (not approved yet).
  - **Before**: a circuit was only a name typed on sections
    (`circuit_name`), nothing else: no object, no id, no description, no
    list (an empty circuit could not exist), no rename other than retyping
    the name on every path. The connections made the *conductor paths*
    (sections joined by jumpers or a shared wire point; one phase each);
    the name grouped any paths, connected or not — phases of one circuit
    are never connected, so the grouping was by name alone. The app kept
    one name per conductor path when a section's name was typed (its path
    followed) and when a jumper joined two paths (the window asking which
    name, phase and system stay); files could still hold a path with two
    names: the example line's branch S3 is tapped on S1 by a jumper and
    named *Branch* while S1 is *Example 1*. A name could group paths of
    several systems, and `--circuits` took names only (a name matching
    nothing checked nothing).
  - **Now**: `circuits` in the line file (version 7, covered by the same
    unreleased raise as the height datum and attachment-point links; OHL
    XML `<circuit>`; the project's settings row `circuits`): id, name,
    optional system, description. Members are conductor paths, stored as
    before on their sections (`circuit_name` = the circuit's name), so
    splits, joins and section renames need nothing else.
  - **The connections rule** (refused when a change brings it; a file
    written with it is read and it is noted in the Circuits table): a
    conductor path is in one circuit; a circuit holds one conductor path
    per phase (system and phase label) — two paths of one phase that are
    not connected are two conductors, so two circuits; a path of another
    system than the circuit's; a repeated or empty name or id. **No
    merge**: neither a merge command nor a rename to another circuit's
    name; two circuits become one only by connecting them (a jumper, a
    wire strung through), which makes their paths one. *Which name stays*
    then: the jumper's window asks, as before (the circuit whose name is
    not chosen keeps its paths that are not connected, or is left with
    none — kept, noted — and the user may delete it). *Grouping paths of
    different phases* (L1, L2, L3 of one circuit, never connected to each
    other) stays allowed: that is what a circuit is — **open question**
    for the owner below.
  - **Editing**: the **Circuits** table (Sections menu): add (empty
    allowed), rename (members follow), system, description, delete
    (members in none, confirmed), members taken out and put in (a path
    still in another circuit refused unless taken out in the same
    Apply); every Apply one command, one undo. **Assign to circuit…** in
    Properties on a selected wire: its whole conductor path (one wire at
    a time: the views select one object). The Conductor paths table's
    name column is now **circuit** (a new name makes the circuit, in the
    same command; contradictions refused) and its paths are labelled P1,
    P2… (were C1…).
  - **Migration**: files without `circuits` are read into one circuit
    per distinct name on their sections, in their order, id the name
    (`-2`… when taken), no system (most conservative: no constraint
    added). The sections are unchanged, so everything grouped by circuit
    — safety volumes per circuit, circuit-to-circuit rules, phase
    distance, wire colours — gives the same results (example lines
    tested; the example's *Example 1* / *Branch* path noted, not
    changed).
  - **`--circuits` (`ohl zones`) and `--circuit` (`ohl safety-volume`)**
    take an id or a name; an unknown one is refused, the circuits listed
    (most conservative: a typo no longer checks nothing). Rule files'
    `circuits` keep taking names, unchecked.
  - **Open questions**: does "unconnected paths are not grouped" mean
    only paths of one phase (as built), or should a circuit's phases also
    have to run together (the same end structures)? Should the circuit
    a connection leaves empty be deleted with it? Should the section
    editor and the Sections table refuse a change that leaves a path in
    two circuits (today only the circuit commands refuse it)?
- **The jumpers' band margin: a value of its own, default 0, entered by
  hand** (owner decision 7). Jumpers' swing bands get their own margin
  (`positions::JUMPER_BAND_MARGIN`, **0 m**: no widening), separate from
  the spans' band margin (0.15 m, unchanged). Shown beside the band
  margin wherever the arc is used — the Vertical and Radial windows and
  the safety volumes (`check_help::jumper_band_margin_ui`, its hover text
  quoting the measured worst excess on the example line's jumper, 0.293 m
  at 230 Pa, and that a value is the user's to enter for their jumpers);
  in the vegetation panel shown greyed, as the danger-tree check leaves
  the jumpers out (a choice of the agent: the field kept visible as asked,
  but not editable where it has no effect). Rule files
  `[wind] jumper_band_margin` (written only when not 0), `--jumper-band-margin`
  on `ohl zones` and `ohl safety-volume` (not on `ohl lidar vegetation`:
  no jumpers there), saved runs (`jumper_band_margin`; a safety-volume run
  saved before reads its `band_margin`, as its jumpers then took it).
  Engine: `WindSteps::jumper_band_margin`, `PositionOptions::jumper_band_margin`,
  `BandMargins { span, jumper }` (`WindSteps::margins()`),
  `run_zone_check`'s arc and `zone_arcs` take `BandMargins`; the safety
  volumes' wind text says both ("band margin 0.15 m, jumpers 0.00 m").
  Labelled in the theory a **Simplification** (on the user's
  responsibility; exact: the wind steps): with 0 the jumper's band is not
  conservative between its blowouts. **Results change**: jumpers' bands
  are 0.15 m narrower by default (clearances to them 0.15 m larger, the
  safety volumes' jumper bands 0.15 m thinner); validation
  `jumperband-001` 1.25 → 1.40 m, `-002` 0.3797 → 0.5297 m (by hand, the
  margin 0), `jumperband-003` added (0.35 m entered: 1.05 m); the
  jumper test (`tests/arc_band.rs`) asserts the excess over the default
  (0.28–0.31 m).
- **Option: ice and wind per unstretched metre ("as other programs")**
  (owner decision 8). A project-level solver setting (Criteria ›
  Calculation options, the line's `SolverOptions::loads_per`,
  `LoadsPer`): **per stretched metre** (default, exact, as since the
  decision of this morning) or **per unstretched metre** (the former
  treatment, which other programs appear to use — the comparison of a
  real line with another program suggested it). In the finite elements
  only: the line's sections (`solve::Reference::loads_per`, set in
  `solve_fe`; rolling in the blocks alike) and the jumpers' catenary
  pieces (`solve_jumper_in`); the closed forms (ruling span, the
  sag-tension job) keep the stretched metre, and the wire FE's own
  section model (`ohl_wirefe::FeSection`, the `ohl sagtension` job's FE)
  has no option (an open question). Line file `solver.loads_per`
  (`stretched` / `unstretched`, written only when not the default; in the
  project's line settings), OHL XML `<solver loads_per>`, `ohl line
  --loads-per`. Results say which was used: `ohl line` prints
  `calculation: ice and wind per … metre` before solving; the app's
  results keep it in their revision (`Revision::loads_per`, the line's
  hash taken without it) and a change marks them out of date naming it
  ("the calculation options (now ice and wind per unstretched metre …)");
  saved check reports (vegetation, safety volumes) keep `loads_per` in
  their parameters, and one opened later is out of date when the line's
  option differs. Theory: the exact one the default, the other a
  **Simplification** for comparison (about \\( T/EA \approx 0.1\\,\\% \\) less
  ice and wind: slightly less sag and swing, on the unsafe side).
  Validation `tbice-002` (tbice-001 with the option: 42551.16362 N, sag
  7.715133902 m — tbice-001's value before this morning; hand check: the
  supports carry the ice on the unstressed length, 8762.04 N). Measured
  on the example line and the crossing line (every span, the five
  example cases; the wire's mass given at its 15 °C stringing temperature
  so that the weight's temperature change is neutral) against the program
  before this morning's change: per unstretched metre reproduces it
  exactly to the spans CSV's digits (0.01 N, 0.1 mm) and the jumper's
  sag; per stretched metre +0.03 to +0.07 % on H in the iced and windy
  cases (max tension up to +38.7 N), sags −3.1 to +1.4 mm, still air
  unchanged.

- **Attachment points in the applications: the Link tool and the
  rigid links drawn** (plan steps 5–8 of O3 item 6, the owner's
  addition of 2026-10-21 "a link tool and visible rigid links"). Built,
  in active development (not approved yet).
  - **OHL-Structure**: the Joints table's **kind** column (auto /
    attach / support, one undo step; a joint carrying a point is not
    made a support joint); attach joints ringed orange and support
    joints marked green in the 3D view; attachment points and strings
    stay editable there (owner's answer 1), the Attachment points table
    giving each point's joint, offset (typed) and its length; **Point
    here** only on a joint that is not a support joint. The FE results
    list each offset's moment (`CaseResult::points`, from
    `ohl_structures_fe::PointLoad`), applied or left out, and offer
    **Make it a frame joint**.
  - **OHL-Design**: the structure screen's Attachment points table
    (joint, offset x/y/z, length, linked or own joint); a dragged point
    keeps its joint and changes its offset, released on another joint
    it is relinked there (structure screen and the 3D view's handles —
    a point on a joint was not dragged before); unlinked points drawn
    red, listed in the structure's Properties; a new string's point on
    a typed structure offered only the type's attach joints; Replace
    structure's **Attachment points** list (exact taken, else the
    nearest attach joint offered with its distance and moment arm, the
    user choosing per point; Replace waits until every point has a
    choice).
  - **Link tool** in both applications (OHL-Design: Tools › Structures
    and the inventory; OHL-Structure: Geometry › Link tool and a Link
    button): a point, then a joint — one undoable command
    (`LinkPoint` / one file edit). **The point is linked where it is**:
    its distance from the joint is kept as its offset, so a link never
    moves a point (conservative: nothing moves without being dragged).
    The joints offered are every joint but support joints (as the
    engine's `linkable_joints`; a joint without a kind becomes an attach
    joint), attach joints drawn brighter; the joint under the pointer
    shows the offset, its length and the **moment arm** (its length
    across the vertical: the arm of a vertical load).
  - **Rigid links drawn** (`ohl_line::links::rigid_links`): magenta
    rods, their own layer (OHL-Design: View › Layers; OHL-Structure:
    View › Rigid links), none at zero offset, clickable (they select
    their attachment point; Properties or the view show its joint,
    offset, length and moment per load case). Structures are drawn once,
    not per weather case: every drawn rigid link is picked.
  - **Choices made where the task left them open (the most
    conservative, for the owner to confirm)**:
    - In OHL-Design a tool acts on the tool click (a right-click by
      default), so the Link tool's two clicks are tool clicks and the
      right-click cannot also cancel: **Escape, or the tool click on
      anything but a ringed joint, cancels** (in OHL-Structure and on
      OHL-Design's structure screen, where a left-click picks, the
      right-click or Escape cancels, as asked).
    - Replace structure: a point not exactly on a joint of the new type
      gets **no default**, even when the new type has a point of its
      label (that choice is offered, "as the new type has it"); removal
      is not offered for a label the new type has (its point stays).
    - **Make it a frame joint** makes every member meeting the joint a
      frame (their cross-sections need iy, iz and torsion; one undo
      step); no other member is changed.
    - The moment per load case in OHL-Design's Properties is r × F of
      the point's factored force (Checks › Structure loads), in the
      structure's axes, z up: the moment the analysis would apply at a
      frame joint, before any analysis.
    - Structure templates and `ohl structures derive` are unchanged.
      A template's level-1 points are their own attach joints when read
      (tested). `derive` writes version-1 `[[attachment]]`s: their
      points name no joint (they are not unlinked — the analysis finds
      them by their attachment, as before — and are not drawn red;
      Properties says so); writing version-2 points linked to attach
      joints instead is left to the owner. A line written without links
      (an importer's, a plugin's) still reads and resolves as before
      (tested). Red and "unlinked" mean the `unlinked` flag (as the
      loads), not merely no joint named.
    - Unlink in OHL-Design is in the point's Properties (Unlink, and the
      menu of its heading); the 3D view's right-click stays the tool
      click.

- **Attachment points linked to typed joints — engine** (owner's
  answers of 2026-10-21 to the plan, O3 item 6). Built in the engine and
  on the command line; the Link tool and the drawn links in the
  applications are next.
  - **Joint kinds**: only `attach` and `support` (`[[node]] kind`,
    `.ohls` 6, the unreleased raise); a joint without one has its kind
    worked out (attach when a point is linked to it, support when it has
    a support). A point on a support joint is refused.
  - **Every attachment point is linked to a joint**: its node; along a
    member, one joint per place; a stick figure's point is its own joint
    (id = its label; `-a`, `-a2`… when a node has it). A point given by
    position exactly at a node (1 µm) is linked to that node — before,
    its load was left out of the finite-element check with a note; now
    it reaches that node. Several points may share a joint (the "one
    point per joint" refusal goes).
  - **A point along a frame member splits the member** for the analysis
    (answer 3): a node there, pieces of its properties, each buckling
    over the whole member's effective length (so no capacity changes);
    results per member, its usage the worst piece's. Along a truss
    member the force is shared to its ends as before. Changes the
    results of points along frame members (the member's own bending
    under the point is now in them); `every-profile.ohls` is the only
    shipped file with one.
  - **Offset = rigid link** (answer 9): the force at the point goes to
    the joint as the same force plus r × F where a frame member meets
    it; where only trusses meet it the moment is left out with a note
    naming the point and asking for a frame joint (a Simplification,
    marked in the theory page). OHL-Structure's notes name every
    offset's moment.
  - **A point on no joint of the members** keeps a structure with
    members from a full design (answer 6).
  - **Line model**: `StructurePoint` `joint`, `offset`, `unlinked` (line
    file 7, project schema 5, OHL XML: the unreleased raise, answer 8);
    nothing written before changes when read. An override written before
    takes its type point's joint, the offset the exact difference (it
    stays where it was); a point of its own at a joint is linked to it,
    elsewhere it is unlinked. A written position is kept while it agrees
    with joint + offset to 1 µm (round trips move nothing; the typed
    example resolves and solves bit for bit, links written or not).
  - **Unlinked points** keep their position; the structure's
    finite-element check is refused (usage infinite, the reason said)
    until they are linked (answer 12). **Conservative:** a failing
    check rather than a pass with a load left out. An attachment point
    no joint of a separate model (`.ohlfe`) carries is refused the same
    way (before: left out with a note).
  - **Matching** (answer 7): exact by default (a joint at the same
    position, 1 µm — positions, not a tolerance); otherwise the nearest
    attach joint is offered with the distance kept as the offset (two
    equally near: none offered); the user chooses per point. No other
    tolerance. Joint ids are not matched across types (a type's `J1` is
    not another's).
  - **Commands**: `link_point` (also relink), `unlink_point`,
    `relink_points`, `replace_structure` `points`; each one undoable log
    entry. `ohl structures points` (links, or the matches of a type
    swap with `--to`) and `ohl structures link` (`--joint` and
    `--offset`, `--nearest`, `--unlink`) on line files.
  - **Not done yet, most conservative choice made**: a structure without
    a type gets no `joints` list (its points stay their own joints; the
    placed editor would drop one until it learns it); OHL-Structure's
    analysis from the line takes the type's links (a placed structure's
    own relink is applied by OHL-Design's structure check only); the
    pole check takes the type's point positions as before.
  - **"Structure point"** is retired; texts say attachment point.

- **One name per object: the glossary** (owner decisions on the
  proposal of 2026-10-21, roadmap O3 item 2: "same object, same name
  everywhere; unify"). The terms are in the
  [Glossary](../user-guide/glossary.md) (Help › Glossary in both apps),
  with drawings generated by `tools/glossary_figures.py`. Decided:
  - **conductor path** for sections joined end to end, one phase of one
    system, with its starts and ends (was "circuit"); **system** stays the
    electrical system. Found while renaming: "circuit" also names a
    *group* of conductor paths, those sharing a `circuit_name` (e.g. the
    three phases of *North*; "per circuit", "circuit to circuit",
    `--circuits NAME`). That meaning is left as "circuit" until the owner
    decides its term (most conservative choice: nothing renamed into a
    wrong meaning) — decided the same day: "circuit" it stays, an object
    of its own (see *Circuits* above);
  - **wire type** for a library entry of the wire library (was "cable");
  - **file keys stay** as they are (`cable`, `circuit_name`, `wires`,
    `[[section]]`…); the glossary gives each term's key; old names are
    read wherever they appear in files or command-line flags;
  - **set** stays: the number of the wire point a wire is connected to;
    the structure templates' "slot" becomes "set" (`slot` still read);
  - **structure template** (was "pylon template"), the format id
    `ohl-pylon-template` and the folder `pylon-templates/` still read;
  - **cross-section** and *cross-section library* for members (texts
    only; `.ohlsec` and `[[section]]` unchanged);
  - **stage** for a part of a project done in turn (was "phase" of a
    project), so that "phase" is only the electrical phase;
  - **no string types**: "I", "V", "V + I" and "TAK" are combinations of
    single strings (a string is one straight member from a top to a wire
    point), not types. The glossary and the one section explaining the
    combinations may name them once for recognition ("Combinations other
    programs name separately … need no type of their own here"), never a
    label or a preset. A two-string assembly in `.ohli` is written
    `legs = 2`, `kind = "v_string"` still read;
  - the German terms as proposed, those marked *(to confirm)* kept so;
  - the plugin interface names (WIT, Python) unchanged for now;
  - **not renamed: "structure point"**. The owner's target is a model
    change, planned separately: in OHL-Structure the structure has typed
    **joints** (*attach*, *foot*, …); in OHL-Design an **attachment
    point** is an object of its own, editable there, linked to an attach
    joint, strings hang from attachment points, and they are matched to
    the new structure's attach joints when a structure changes. The
    glossary carries both entries marked *planned (2026-10-21)*.
  Formats (old names always read): `.ohli` version **3** writes two
  strings to one clamp as `legs = 2`; structure templates' format id
  `ohl-structure-template` and folders `structure-templates`,
  `Structure templates`, `StructureTemplates/`; the sag-tension job's
  `[fe.two_strings]`; the command line's `--wire-type`
  ([data model](data-model.md#file-headers)).
  The docs' links and anchors are checked by `tools/check_doc_links.py`
  (mdbook does not check anchors).

- **The LiDAR corridor of the lines on the ground beside the line**
  (owner decision: "The LiDAR corridor for the ground lines should be
  modifiable, but after some point it doesn't make much sense
  anymore"). View › Lines on the ground beside the line has a field
  **LiDAR corridor ±** (m from the spans' chords, as the model's
  `--corridor`). *Default: the model's corridor (60 m)*, not "the
  distance + a margin": it reads nothing more, the lines are as before,
  and a margin rule would read the whole cloud again each time the
  distance is changed; when the lines go beyond the corridor the menu
  says so and gives the width that covers them (the farthest point of
  the lines from the chords + 2 m, to the next 5 m). *Wider*: a LiDAR
  ground of the lines' own, built in the background from the ground
  classes of the whole cloud exactly as the model's (lowest point per
  1 m cell, triangle sides ≤ 50 m), kept per width (two) and built
  again when the cloud's ground points, the ground classes or the spans
  change. *Separate from the model's ground*: the codebase shares one
  ground (`Model::ground`) between the views and the checks, so the
  lines get their own instead of widening it — the checks' ground, and
  their results, stay identical (tested: the same object, the same
  points). *Maximum 200 m*, from measurements on synthetic clouds of 5.2
  and 11.8 million points along 1.9 km of spans (release): ±60 m 0.24
  million cells 0.6–0.7 s; ±120 m 0.49 million 1.6–1.7 s ~105–140 MB;
  ±200 m 0.77 million 1.8–2.1 s ~175–190 MB — about 90 MB and a second
  per km of line at 200 m, 3.3 times the model's ground, while a line's
  corridor survey rarely holds ground points farther out. A wider value
  in a settings file is taken as 200 m. A view setting
  (`ViewPrefs.ground_lines.lidar_corridor`), remembered per project on
  this computer. Built, in active development (not approved yet).
- **The swing band's margin is editable everywhere it is used** (the
  owner): default 0.15 m (`positions::BAND_MARGIN`, the measured worst
  excess plus an allowance; **Conservative**), a field **band margin**
  beside the swing arc in the Vertical and Radial clearance windows, the
  vegetation panel and the safety volumes (shared helper
  `check_help::band_margin_ui`: a line, hover text on what it covers,
  conservative, a smaller value less safe and on the user's
  responsibility — a warning below the default, a button back to it);
  rule files `[wind] band_margin` (additive, written only when not the
  default; presets may set it); `--band-margin` on `ohl zones`,
  `ohl lidar vegetation` and `ohl safety-volume`; saved runs record it
  (`band_margin`; older runs read 0.15 m) and are out of date when it
  changes with the arc on. Engine: `WindSteps::band_margin`,
  `PositionOptions::band_margin`, `SwingArc::with_margin`,
  `run_zone_check`'s arc as `Option<margin>`; a negative margin refused.
  Results with the default are unchanged. Validation: the band cases
  take an optional `band_margin` (the default cases unchanged),
  `swingarc-005` with 0.4 m (clearance 3 − 0.4 = 2.6 m by hand).
- **Jumpers get their own swing band** (the owner): a jumper's positions
  blown left, in still air and blown right form a band like a span's,
  about the axis through its two end clamps (as solved blown to the left,
  the first solve — the axis its positions are placed on), from the
  smallest to the largest radius, widened by the band margin
  (`positions::jumper_band`). Used wherever jumpers are included: the
  safety volumes, and the vertical and radial checks for the rules whose
  `wires` take jumpers (before: "jumpers have no swing arc", checked at
  their solved positions). A jumper without a band (not solved blown to
  a side, or not running steadily along its clamps' line) keeps its
  solved positions (noted). **Measured** against steps of 10 Pa from both
  sides (`tests/arc_band.rs`) on the example line's jumper, both windy
  cases (158 stepped positions): up to **0.293 m** outside the band
  (high wind, blown right at 230 Pa, inside the inner radius; rime ice
  and wind 0.127 m), against 2.54 m from the arc alone at the largest
  radius. **This exceeds the default margin by 0.143 m**; the margin is
  not widened without the owner's approval (reported: about 0.35 m would
  hold it with an allowance as the spans'; or a margin of its own for
  jumpers). With the axis through the still-air clamps the excess was
  0.375 m, so the blown-left clamps are kept. Validation
  `jumperband-001`, `-002` (by hand).
- **Fixed: a rule's voltages and wires were not applied to the swing
  arcs** in the vertical and radial checks (`zone_arc::arcs_of_rule`
  tested the length of `checked_parts`, which keeps a left-out wire
  without points): every rule checked every span's arc, also of other
  voltages or of the sections in a jumpers-only rule. Found while adding
  the jumpers' bands; results with the arc and rules limited by voltage
  or wires change (fewer, correct findings).
- **Wires without a system as one group** (the owner): in the safety
  volumes the scope by Systems lists a row **(no system)** — the earthed
  wires and any section without a system — ticked like a system
  (`safety_volume::NO_SYSTEM`; `ohl safety-volume --system none`, or
  `"(no system)"`; `none` means a system when the line has one of that
  id or name); per system they form one volume "(no system)" instead of
  one per circuit; their distance by voltage stays at 0 kV. This
  replaces the agent's choice of earlier today (left out by system, per
  circuit). "By system" exists nowhere else (the clearance checks choose
  by voltage and circuit), so nothing else changes.
- **The weight from the data sheet's temperature; stringing at 20 °C by
  default** (owner decision 4). A cable's mass per metre is its data
  sheet's, at the data sheet's reference temperature: the wire library's
  new optional `reference_temperature` of a cable (wire library version
  2), else 20 °C. The calculations take it per metre of unstressed cable
  at the stringing temperature, \\( w_s = w_{ref}\\, e^{-\alpha (t_s -
  t_{ref})} \\) (multiplicative: the linear finite elements, the closed
  forms, jumpers at their own temperature, the spotting at its
  condition's), or \\( w_{ref} (1 + \varepsilon(0, t_{ref})) / (1 +
  \varepsilon(0, t_s)) \\) in the stress-strain curves' additive form
  (the ruling-span method, the finite elements of a cable with curves);
  from there the weight is conserved as before. The engines keep their
  API (the weight as given at the known state); the line
  (`solve::strung_cable`, `jumper::cut_cable`), the CLI's sag-tension job
  and the spotting convert. **Defaults changed** to 20 °C (from 15 °C): a
  new section made in the app with no section to copy, a new jumper made
  by the Add jumper tool or the Jumpers table
  (`ohl_line::DEFAULT_STRINGING_TEMPERATURE`). **Not changed:** values
  given in data (every file keeps its stringing and jumper temperatures);
  a jumper temperature left out of a line or OHL XML file still reads as
  15 °C (files were written with that meaning; OpenOHL always writes the
  value) — *a choice of the agent, open to the owner* (a file version
  raise could make it 20 °C for new files); weather-case temperatures
  that are not stringing temperatures (`ohl line --temperature` 15 °C,
  the app's default still-air case at 15 °C, the survey temperature of the
  wire fit, the spotting window's design condition at 80 °C) are left as
  they are. Effect: the textbook cable strung at 10 °C weighs 0.020 %
  more than its 20 °C data sheet value; the example line (strung at
  15 °C) +0.009 % on H in still air, sag +0.9 mm at most. Validation
  `tbwref-001…002`; the line-model cases' references updated through the
  reference scripts, tolerances unchanged.
- **Ice and wind per stretched metre in the finite elements** (owner
  decision 5). The FE carried ice and wind per metre of unstressed length,
  about \\( T/EA \\) (10⁻³) too little (a stated Simplification, on the
  unsafe side). Now each cable element carries its own weight per unit
  unstressed length and its ice and wind per metre of its stretched
  length (`ohl_wirefe::CableElement::stretched_load`), applied as
  \\( \mathbf q_s\\, L/L_0 \\) with the element's exact stretched length
  (`CableProperties::stretched_length`); the solve is a fixed point on
  the elements' stretches (`Model::solve`: settled when no stretch
  changes by more than 10⁻¹¹, three solves in practice; exit clause 20
  solves, then an error naming the element). A section in the blocks
  shares its length out with the same loads; jumper pieces solved as
  catenaries too. The closed forms were checked: they already carry ice
  and wind per metre of the stretched catenary. What remains is stated as
  a smaller Simplification: the ice and wind spread evenly at each
  element's mean stretch (3.3·10⁻⁷ of the tension on a level 300 m span
  under 12 mm of ice against an integration following the local stretch;
  FE and closed form 4.2·10⁻⁷ apart there, 2.8·10⁻⁸ on 100 m — the owner
  asked for 1e-7 between them: met on short spans, not on 300 m; an open
  question). Effect on the example line: iced and windy cases +0.03 to
  +0.07 % on H, sags within ±2.6 mm; validation `tbice-001`, the changed
  cases in [Textbook and published cases](../validation/textbook.md).
  Cost: a 60-span section in the blocks 32 → 39 ms (glaze ice), 40 →
  49 ms (rime ice and wind), 44 → 55 ms (high wind); still air unchanged.

- **Lines on the ground beside the line without a terrain surface**
  (owner request: "make sure the line on the side of the conductors also
  works if the TIN is not loaded, purely based on the LiDAR, or at least
  ask to create a TIN when it is not given"). Checked on the example line
  with a synthetic LAS tile (a sloped, waved ground with ±5 cm noise,
  vegetation 4–14 m above it), no terrain surface, 2000 points shown:
  the lines already lay on the ground the project builds from the LiDAR
  ground classes when it opens — every ground point within the corridor
  of the spans read from the whole cloud (not from the points shown; the
  same ground the checks use, the lowest point per 1 m cell, triangle
  sides ≤ 50 m), within the noise of the synthetic ground, the vegetation
  ignored (`ground_lines_tests.rs`). What was broken, and is changed:
  (1) the active terrain surface deleted, the lines fell back to the
  alignment's heights although the project has LiDAR — with no ground,
  the ground is now built again from the LiDAR ground classes in the
  background, once per revision of the ground points and set of ground
  classes (also after a class scheme with ground classes is chosen;
  `lidar_edit::frame`), and a LiDAR ground built in the background no
  longer replaces a terrain surface made the ground meanwhile; (2) the
  menu said "on the ground from the LiDAR points" after a terrain surface
  was made the ground in a project opened with the LiDAR ground; it now
  says **Ground: TIN (name)**, **LiDAR ground classes (n points)** or
  **alignment heights only** from what the ground is. Added: the
  stretches of the lines with no ground under them are counted and said
  (the lines broken there, as at a surface's edge); with no ground under
  the lines and a point cloud, the menu says why (no ground class in the
  scheme, no point of a ground class, none in the corridor, or the ground
  does not reach the lines) and offers **Build a terrain surface from the
  LiDAR…**, which opens Terrain › Terrain surfaces at Build from LiDAR;
  the surface built becomes the ground and the lines are draped on it.
  The LiDAR ground keeps the model's corridor (`--corridor`, 60 m): lines
  farther from the conductors break there (an open question: whether
  the corridor should widen with the distance chosen). Display only;
  nothing stored changes.
- **Safety volumes chosen by circuit or by system** (owner): the window
  chooses its wires from the circuit list or from the system list (a
  toggle "Circuits | Systems", a tick list with All / None) and makes one
  volume per wire, per circuit or per **system** (`Grouping::System`,
  `per = "system"`; every circuit of one electrical system together).
  The distance by voltage still uses each wire's own system voltage. The
  engine takes the systems chosen (`SafetyParams::systems`, by id; the
  CLI's `--system` takes an id or a name and `--per system`); a wire's
  system is that of its section, a jumper's that of its `from` section.
  **A choice of the agent, open to the owner:** earthed wires have no system;
  per system they make one volume per circuit, and a choice by system
  leaves them out (tick their circuits instead). The saved run records
  the choice (`scope`, `circuits`, `systems`, `per`; runs saved before
  read as circuits). One engine change outside the app, named in the
  owner's request: `Grouping::System` and the systems filter.
- **Safety volumes: scope by Systems | Circuits | Spans** (owner,
  addition of the same day): a choice of the scope, then a tick list of
  the line's systems (name, voltage), circuits (name, system, voltage) or
  spans (section, span number, the two structures, length in plan), with
  a search field filtering as you type (name or id, substring, case
  alike) and All / None / Invert acting on the rows shown only, a count
  "n of m ticked", in a scroll box of fixed height. One shared widget
  (`ohl-app` `tick_list`). A span picked brings every wire on it (within
  a circuit or system filter); a jumper comes along when the spans at
  both its ends (its `from` and `to` attachments) are picked — the
  engine's positions hold no jumpers yet, so this takes effect once they
  do. Engine: `SafetyParams::spans` (section, index from 0), CLI `--span
  SECTION:N` (N from 1), saved `scope = "spans"`, `spans`.
- **The distance rules are visibly editable** (owner): everywhere the
  distance editor appears (the rule editor, the clearance windows' rule
  tables and Add a rule, the vegetation panel, the safety-volume
  distance) it shows a form selector "One distance | Table by voltage |
  Base + per kV", every value as a framed field with its unit in place
  (no value in a menu any more), table rows with × (remove), + Add row, Sort
  by voltage and a hint on the order under a "voltage up to / distance"
  header, and a live line ("At 220 kV: 7.00 m; at 380 kV: 9.00 m", or the
  formula written out). Read-only places (a preset's rules before Apply)
  show the same editor greyed, marked "(preview — Apply to edit)". One
  shared component (`ohl-app` `requirement_ui`). **Switching forms**:
  a form left and come back to unchanged is restored; otherwise one
  distance and a base take the largest distance at the line's voltages
  (conservative; before: the first row's), a table from a formula gets
  one row per voltage of the line. The values written to files are
  unchanged. With the editors in place the Vertical / Radial clearances
  windows scroll (they could grow taller than the screen), and the
  safety-volume window has Run and its results at the top, as they do.
- **The safety-volume window explained like the clearance windows**
  (owner): five groups with the shared headings and user-guide links
  (wire positions, distance, what is included, mesh, export), a line and
  a hover text per value, drawings of the distance around a conductor,
  the wind positions and arc, the grouping and a mesh where two tubes
  meet. The wind choice is the shared one (two ticks).
- **Lines on the ground beside the line: a set of weather cases** (owner):
  the lines are the plan envelope over every weather case chosen in the
  menu, **both wind sides** where a case has wind (whatever side Display
  weather cases shows: the other side is solved for the lines in the
  background, `ground_lines::GroundLines`; until it is, the menu says so
  and the lines take what is solved). The default is the case shown
  (followed as another is shown). Chosen with ticks in the menu (it stays
  open): All, None, the case shown, each case by name. Remembered per
  project as a view setting (`ViewPrefs.ground_lines.shown_case`,
  `cases` by name; settings of before read with the case shown). The
  lines kept are keyed by the set of cases (the case shown counts only
  while ticked). Replaces "the case chosen as shown only" decided on
  2026-10-20. The envelope over both wind sides lies farther out than the
  side shown alone: on the example 10 m from the conductors, the right
  line moves more than 1 m out in high wind (600 Pa), the wind from the
  left blowing the conductors right.
- **The lines on the ground include the jumpers** (owner): their solved
  positions in each case chosen count as conductors (each subconductor),
  so the lines go on round the structures. Checked on the example at the
  Y pylon T3, a dead-end of the branch with its jumper across the pylon
  and an angle tower of the main route: 2 m away, the branch's lines
  reach back to T3 with the jumper (they would begin 4.5 m out without
  it), and the main route's lines go round T3 in one piece, crossing
  themselves nowhere (`ground_lines_tests.rs`).
- **The near side is the right** (owner, confirmed): looking along
  increasing stations, for the sheets' corridor sides (`[corridor]
  near_side`, default `"right"`) and the profile sides of the lines on the
  ground (right solid, left dashed). Kept as built on 2026-10-20.
- **The lines on the ground: default distance 10 m** (owner, confirmed):
  as built on 2026-10-20 (a display value, no rule behind it).
- **A project height datum** (owner): the project's vertical datum, a
  name and an optional EPSG code of its vertical system (`LN02` /
  EPSG:5728, `LHN95`), optional — `ohl_line::HeightDatum`,
  `Line::height_datum`. Set in Terrain › Coordinate system › Define…
  next to the coordinate system (one line-settings edit, undone as any)
  and from the command line as the coordinate system is (`ohl project
  apply`, a `put` of the `meta` row `line`). The sheets' `{height_datum}`
  takes it first, then the coordinate system's compound vertical part,
  then the template's `[if_empty]`. Shown in the coordinate-system window
  and menu and in the project's properties. Format: line file version
  **7** (OHL XML follows: `height_datum`, `height_datum_epsg`), project
  `session_schema` **5** — older files and projects read unchanged, with
  none; an empty datum is refused. Nothing is converted with it
  (informative, like the coordinate system): reprojecting keeps it.
- **Presets are per check** (owner, "test dependent": the vegetation
  preset's 80 °C and 60 °C + 200 Pa are not the ground-clearance cases).
  Each place lists only the presets for its check — the vegetation panel
  those with `[vegetation]`, the Vertical window those with a vertical
  rule, the Radial window those with a radial rule, the rule editor all —
  and applying one takes only that check's part: the Vertical window its
  vertical rules and the cases they name, the Radial window its radial
  rules and theirs, the vegetation panel its `[vegetation]`, `[wind]` and
  its own cases; no case only another check uses is created. A rule file
  may declare its checks (`checks = ["vertical", "radial",
  "vegetation"]`, optional, top level; files without it classified by
  content), and `[vegetation]` may name its own cases (`cases`, optional).
  So one big preset may carry many checks in a row (dozens of rules, each
  with its own cases); the previews group the rules by target and give
  each rule's cases. Decided here (left open): a rule naming no case
  needs none of the preset's (it applies to the project's); without
  `[vegetation] cases`, the vegetation check's are the preset's cases no
  rule names (all of them without rules); a rule with both distances goes
  to both windows; the rule editor takes every rule but not
  `[vegetation]`. ([plugins](plugins.md#presets-per-check), [data
  model](data-model.md))
- **A preset's weather cases are matched by physics first, then by
  name** (owner: "if the ones needed already exist, do not recreate
  them, but use the existing ones"). **Replaces** the name-only rule of
  2026-10-20, which filled projects with duplicates ("80 °C" next to
  "Utility vegetation 80 °C" of the same weather). Wherever a preset is
  applied (the rule editor's Load preset…, the Vertical / Radial
  clearance windows, the vegetation panel): a preset case equivalent to
  one of the project's uses it — the preset's rules rewritten to name
  the project's case, the case ticked for the run, nothing created; with
  several equivalent, the first in the project's order, said in the
  preview; only a case with no equivalent is created, as
  `"<name> (preset)"` when the project gives its name to a case of other
  values. The previews list "uses your case X" / "creates Y".
  Equivalent (`WeatherCase::same_physics`): temperature, ice (none on
  both; or the same thickness and density; or the same load and density;
  glaze ice when no density is given), wind pressure, with wind its side
  and air density factor, ambient temperature — within 1e-6 relative;
  never the name, description or tags. Decided here (the request left it
  open), each on the side of creating a case rather than reusing a
  different one: ice as a **thickness and as a load are never
  equivalent** (the weight of a thickness depends on the wire); an air
  density factor or ambient temperature given is never equivalent to one
  not given (the project's default could differ); a rule's `*` `?`
  pattern matching a renamed case becomes the names it matched. No
  command-line path writes a preset's cases into a project, so the CLI is
  unchanged. A "Merge duplicates" helper for existing projects is not
  built (proposal in the roadmap). ([user
  guide](../user-guide/design-codes.md#a-presets-weather-cases),
  [plugins](plugins.md#matching-a-presets-weather-cases))
- **The swing arc becomes a band** (the owner): from the smallest to the
  largest radius of the left, still and right positions at each place
  along the span, so that it is on the safe side also for targets between
  the axis and the arc. The radial distance is 0 inside the band, else to
  the nearer bounding surface (outer or inner radius, or an end edge);
  vertically the band's lowest or highest point over the plan position
  (on its two circles or its two end edges). One code
  (`positions::SwingArc`) for the danger-tree check, the vertical and
  radial clearance checks and the safety volumes: all three change
  together. The ground under it: the band's boundary (outer and inner
  surfaces every 2 cm, the end edges every 2 cm). Measured with steps of
  10 Pa (both sides) on the example line and the crossing line, both
  windy example cases: the band alone holds the steps to within 0.089 m
  (the arc alone at the largest radius: 1.533 m), so it is **widened by a
  margin of 0.15 m on every side** (`positions::BAND_MARGIN`: the worst
  excess plus an allowance of 0.061 m; every distance to the band that
  much smaller): no step lies outside it. On the example line every
  clearance to the ground on the arc and every danger-tree fall margin is
  0.150 m smaller; points between the axis and the arc are now found
  (the wire at half the wind: 0.47 m inside the arc before, inside the
  band now). The validation cases' references are new hand derivations
  (`swingarc-002…004`, `arcclear-004`, `arcclear-006`,
  `safetyvolume-002…003`). The ground check on the arc takes about three
  times as long (the inner surface and the edges).
- **The safety volume in IFC is a spatial element** (the owner): an
  `IfcSpatialZone`, predefined type `USERDEFINED`, object type
  `SafetyVolume` (IFC4 has no spatial zone type for it), placed relative
  to the site and referenced in it by an
  `IfcRelReferencedInSpatialStructure` — IFC's relation for a spatial
  zone (non-hierarchical, may overlap; not contained like an element),
  the site aggregated in the project. Property set and georeferencing as
  before. Before: an `IfcBuildingElementProxy` contained in the site.
- **The safety volume includes the jumpers** (the owner): their solved
  positions in each case (blown left and right, still air, every step;
  never on a swing arc), inflated like the wires, so that the volume is
  continuous at the structures (`PositionOptions::jumpers`; the
  danger-tree check leaves them out as before). Structures stay excluded
  (said in the window's note and the theory). Tested: two sections whose
  dead-end clamps are 6 m apart give two pieces at 1 m, one with the
  jumper (validation `safetyvolume-004`, `-005`).

- **The cable's own weight is conserved in every calculation** (owner
  decision: "Wasn't it already the case? Please correct it."). The
  textbook validation found that in the finite elements a span's
  unstressed length grew by \\( e^{\alpha\Delta t} \\) while its load stayed
  per unit of that length, so a hot span weighed more than its cable
  (+0.04 % on H at 70 °C, +0.17 % at 150 °C on the textbook span). Every
  solver path was checked:
  - *already right:* the finite elements under elastic stretch (the
    elastic catenary carries its load per unit of unstressed length);
    point loads, bodies and strings (forces, unchanged by temperature);
    ice and wind (per metre of the real conductor: they act on the longer
    or shorter cable as it is, in the finite elements as in the closed
    forms);
  - *not right, corrected:* the finite elements' thermal lengthening
    (`ohl-wirefe` section model, `ohl-line` sections and rolling), the
    linear models' creep stretch, the stress-strain curves' strain and
    the bimetallic core branch in the finite elements, spread loads and
    jumpers — the own weight is now \\( w/(k\,e^{\alpha\Delta t}) \\) per
    unit of the element's length (\\( k \\) the lengthening without mass);
    the closed-form change of state, tension sections, governing tension,
    loaded spans and the ruling-span (stress-strain) method, which took
    the same load per metre of *stretched* cable in every state, so their
    spans grew heavier with every lengthening, thermal or elastic (+0.25 %
    of weight at 150 °C) — the own weight is now per metre of unstressed
    cable at the known (stringing) state's temperature, \\( w/k \\) per
    stretched metre, \\( k = e^{\alpha\Delta t}(1 + \bar T/EA) \\) or, in
    the additive form, \\( (1+\varepsilon)/(1+\varepsilon_w) \\); creep and
    permanent set add length, not weight.
  A load state now says which part of its load is the own weight and
  which part is wind (`ohl_sagtension::LoadState::weight`, `wind`; the
  validation inputs `weight`, `wind`). Effect on the textbook span:
  closed form −0.12 % on H at 70 °C, −0.28 % at 150 °C, sag 5–6 mm less
  (most of it the stringing state's weight, 0.08 % too heavy before);
  FE −0.12 % at 80 °C on the three-span section, +0.04 % at −20 °C. On
  the synthetic example line (FE, strung at 15 °C): H −0.11 % at 80 °C,
  −0.24 % at 150 °C, +0.05 % at −20 °C, +0.015 % iced; sags within 2 mm.
  With it the closed form and the finite elements agree to 10⁻⁶ on a
  single span, FE and ruling span differ only by the thermal form
  (+0.047 % at 150 °C on 400 m), and CIGRE TB 324 sec. 6.1 comes within
  the printed rounding (tolerance 2.5 N → 0.5 N). 25 validation cases
  changed value (each recomputed by its independent reference; two
  safety-volume cases moved within their tolerances, by 0.005 % and
  0.8 mm); 3 cases
  added (`tbheat-001…003`: a 400 m span at 150 °C weighing what it
  weighs when strung, closed form, FE and ruling span). Left as stated
  Simplifications: in the finite elements ice and wind per metre of
  unstressed (not stretched) conductor, about 0.1 % less ice and wind;
  the closed form spreading the weight evenly over the stretched span.
  The mass per length is taken at the stringing temperature (the
  datasheet's own temperature is not in the cable data).
- **Strain strings without a weight keep the estimated weight when they
  name a 0 N assembly** (owner request, found on a real line: after the
  string catalogue linked imported strain strings to "0 N" library
  entries, the median sag difference to another program's results grew
  from 0.7–2.4 cm to 2–9 cm, +0.29 m on short dead-end spans). A strain
  string whose resolved weight is 0 — its own or its assembly's — counts
  as without a weight, whether or not it names an assembly, and gets the
  weight that holds it where the data puts it; an assembly with a real
  weight gives that weight. The string catalogue
  (`Line::catalogue_strings`) no longer takes strings without a weight
  into the library: they stay unlabelled (chosen over an assembly
  "without a weight": the data model's assemblies always have one, and
  0 N would read as a real weight to every other user of the library).

## 2026-10-20

- **Sheet templates: second round of fixes and the owner's additions**
  (an A1 export of the synthetic line with a title block of 35/55/45/45
  mm):
  - The **title block, revision table and legend lie inside the frame's
    padding** (they sat on the frame's outer edge and crossed its lines;
    right-aligned values were cut). The built-in full-width title block
    moves in by the padding too (5 mm; the profile panel 5 mm lower). A
    text too wide or tall for its cell is made smaller (to 40 %), a
    revision cell's (to 60 %) then shortened with "…"; a note is broken
    at the padding, and one lying over the title block, revisions,
    legend, key map or scale bars is warned.
  - **No empty field leaves a gap**: `{height_datum}` comes from the
    coordinate system (the vertical part of `EPSG:h+v`, a proj4 geoid);
    otherwise every field that comes out empty is written as the
    template's `[if_empty]` says, built in "unknown datum"
    (`height_datum`), "unknown coordinate system" (`crs`), "–" (any
    other); the station range's `ends` and the title lines stay empty
    unless named. The project has no vertical datum of its own (a core
    change, not made; the owner decided one on 2026-10-21, above).
  - **Templates carry the words of every generated text**: the legend's
    entries (`[legend.labels]` by entry kind, with placeholders) and the
    panel titles (`[layout] plan_title`, `profile_title`); English stays
    the default.
  - **Structures band row** `format` (`{name}`, `{type}`, `{type_id}`,
    `{extension}`, `{station}`, `{height}`, two lines at a line break).
  - **The label of the line below the lowest conductor** is placed clear
    of the structures, the wires and the ground (`[conductor_clearance]
    label_at = "auto"`, default; `"middle"` the old place; `"start"`,
    `"end"`).
  - **LiDAR points on the sheets** (owner): the classes chosen (template
    `[lidar] classes`, the export window, `--sheet-lidar-classes`) within
    the corridor (else the line width), as dots in the profile (by
    station and height) and optionally the plan, styled per class or
    group, named in the legend by the class scheme; one dot per class
    and 0.2 mm paper cell (drawing only; no number changes). The app
    draws the points it has loaded (its point budget), the command line
    reads `--lidar` / `--pointcloud`.
  - **The corridor's sides in the profile** (owner): the ground along
    both edges, the **near side the right** looking along increasing
    stations by default (confirmed by the owner on 2026-10-21; the plan's
    lower edge, nearest a reader of the profile under the plan;
    `[corridor] near_side`), each edge in its own
    pen (`near`, `far`) in the plan, the key map and the profile alike,
    both in the legend. `--side-offset` stays as it was.
  - **The centreline** (owner): the ground on the axis always solid
    (`[centreline] ground = false` only leaves it out); the plan's axis in
    `[centreline] pen` (the built-in look kept: grey dash-dot; the
    example: solid black).
- **Lines on the ground beside the line** (owner request: "an option in
  View to draw a line on each side of the lines, on the ground, at the
  displayed weather case and a given distance away from the line"): a
  display option, View › Lines on the ground beside the line, this
  user's view setting (`ViewPrefs.ground_lines`, never in the project,
  not logged, never in a result). The distance is measured by default
  **from the outermost conductor** of the weather case shown, in plan,
  with its wind blowout (both wind sides when both are shown); each side
  from its own outermost conductor; the alternative is **from the
  axis** (the alignment). Geometry: per span, across its chord every 2 m,
  the farthest point within the distance of that span's wires (and the
  jumpers at its ends); round outer corners about the structure in 5°
  steps; the inner loop cut off where the two spans' lines cross (the
  outer envelope, no self-intersection). Draped on the active terrain
  surface (or the LiDAR ground), a point at every triangle edge crossed;
  without a ground surface on the alignment's heights every 2 m. 3D and
  plan; the profile as two side profiles (right near, left far) when
  ticked. Kept for the last eight weather cases shown.
- **Decided here** for the lines on the ground (the most conservative
  choice, open to the owner): default distance **10 m** (a display
  value, no rule behind it; confirmed by the owner on 2026-10-21); with
  several weather cases shown, **the case chosen as shown** only (not
  the envelope of all of them; replaced on 2026-10-21 by a set of cases
  chosen in the menu, both wind sides); spans
  to structures **away from the alignment** and **other project lines**
  are not taken into account; the lines **end at the first and last
  structure** (no round end caps); where the line leaves the ground
  surface it is **broken**, not continued on the alignment's heights.

- **Textbook and published validation cases** (the owner: "start with
  the textbook cases to improve the test suite"): 31 textbook cases
  (`benchmarks/*/textbook-*.toml`) and 16 worked examples of CIGRE TB 324
  (`benchmarks/sagtension/cigre-tb324.toml`), 135 cases in all. Each
  textbook case has a hand derivation in its comments and expected values
  from a new independent reference, `benchmarks/reference/textbook_reference.py`
  (standard library; the cable as the quadrature of its tangent, Newton
  with the quadrature's Jacobian — none of the code's formulas). The TB 324
  cases take the brochure's printed numbers with their printed rounding as
  tolerance; examples with incomplete inputs (Table 3, Table 4), graphical
  or curve-based ones (sec. 6.2, 6.3, Tables 5, 6, 12, 14, 15) are left out
  with the reason ([Textbook and published cases](../validation/textbook.md)).
  Two validation calculations added (runner adapters, no engine change):
  `line.section` (a line's section by its solver, optionally clipped in as
  the app does) and `cables.properties`. No computed number changes. Found
  and now documented as a **Simplification** in
  [Finite-element wire systems](../theory/wirefe.md#elastic-catenary-element):
  the FE's thermal lengthening of a span's unstressed length also scales
  its weight (+0.17 % on H at 150 °C, −0.04 % at −20 °C for the textbook
  span); left for the owner to decide.
- **Safety volumes for work near the line** (the owner's request: "a
  tool to draw a volume around the lines with a given diameter from
  weather cases and a voltage rule, with the wind stepped or arced, that I
  can then export in a georeferenced file; it's for safety of work").
  Built (in active development, not approved yet): Checks › Safety
  volumes and `ohl safety-volume`. The volume is the union over the cases
  chosen and the wind positions (blowouts; steps; arc; both — the
  danger-tree check's positions) of the points within d(U) + margin of
  the conductor's surface, d a requirement by voltage (earthed wires at
  0 kV), one closed mesh per wire or circuit. **No distance is built
  in** (the user's, or a preset rule's radial else vertical distance).
  The mesh: marching tetrahedra on a sparse grid at a level 60 % of the
  tolerance, the cell chosen so that no triangle can enter the envelope
  (proof in the theory), then edge collapses kept only when every new
  triangle stays out of it (exact distances) and the result checked as a
  whole — never inside the exact volume, outside by about the tolerance
  where smooth, more where tubes meet (measured and shown). Exports: DXF
  R12 3DFACE + `.prj`, IFC4 (`IfcBuildingElementProxy`,
  `IfcTriangulatedFaceSet`, `IfcMapConversion` + `IfcProjectedCRS`), KML/
  KMZ (polygons, WGS84, heights kept, no geoid). Additive, small core
  changes: `ohl_crs::Crs::epsg` and `wkt`; the IFC writer's helpers
  shared within `ohl-io`; `--case` takes `--cases` as an alias.
- **Decided here** (not covered by the request; the most conservative
  choice, open to the owner): structures and **jumpers are not
  included**; a wire is a circuit's phase through all its sections; the
  default tolerance 0.05 m; a preset fills the distance from a rule's
  radial distance, else its vertical one; KML as polygons (not a COLLADA
  model) with the line's heights unchanged; the volume element in IFC is
  an `IfcBuildingElementProxy` (object type `SafetyVolume`); without an
  EPSG system no `.prj` is written and no KML (said); meshes of more than
  2 000 000 triangles are saved without their meshes.
- **Measured** (`the_arc_holds_the_steps_up_to_the_rigid_swing`, the
  example line, 600 Pa in steps of 100 Pa): a stepped position lies up to
  1.53 m inside the arc's radius (towards the axis): the swing arc alone
  does not hold the steps' tubes on its inner side — "both" does.

- **Sheet export: the defects fixed** (the review of the sheet export,
  D1–D17): the profile panel takes the whole height; its height range
  fits the ground, the clearance line and the wires' lowest points; tops
  that do not fit are cut (a break mark), and when not even those fit the
  **datum is stepped** at a structure — the vertical scale is never
  changed, the ground always seen, each warned; height labels inside the
  panel; panels and content end at the match lines; a structure at a
  sheet's end drawn whole on both sheets; elevations drawn after the match
  lines, the plan panel grown to hold them; heading "Route id — name";
  the app draws each other weather case from its own solution and takes
  the markers, side profiles, plan scale, logos and the ground rule's
  clearance; PDF logos cropped (no hairlines) and only what is seen
  written; DXF texts in R12's own codes and logos left out warned. The
  sheets' look changes accordingly (height ranges, panel heights); no
  computed number changes.
- **Sheet templates — the owner's answers** (all 14 as recommended):
  extension **`.ohlsheet`** (format `ohl-sheet-template`, v1); templates
  **in the project and in a library folder** (the project's with their
  images embedded); plugin presets **through the WIT and as manifest
  files**; an **own revision list** per project line with a prefill from
  the last version commit; **DXF R12** with SVG logos as outlines, raster
  logos left out with a warning (DXF 2000 later if needed); a **north
  arrow per straight stretch**; a font **embedded only on request**
  (Helvetica stays the default — embedding itself not built yet);
  templates **carry their own texts** (no translation tables); a profile
  taller than the sheet **steps the datum** and warns; **nothing beyond
  the match line** by default, faded as an option; **portrait, long
  formats and a binding edge** yes (the built-in keeps 5 mm margins, the
  example 20 mm); **templates define the band rows** (the export may
  switch them off); **scale bars** in the example template, not in the
  built-in; an **empty required field refuses the export**, naming the
  fields.
- **Sheet templates — the owner's additions** (all template or export
  options, default off or neutral; generic, no third party named):
  (1) **several versions of the line on one sheet**, each in a **role**
  (existing, project, to be removed, decommissioned, other …) styled by
  the template, in plan, profile and elevations, with legend entries;
  chosen in the export window (project lines) and `--version LINE=ROLE`;
  (2) the **corridor** at ± W from the axis in the plan and the key map,
  labelled, the plan at least that wide; (3) **styles by line kind**
  (high / low voltage, telecom, earth wire) combined with the role,
  overhead solid and cable dashed, other lines grey with their label; a
  **legend** made from what each sheet draws, a graphic **scale bar**,
  **north arrows**; every item settable from a template (the keys
  documented), the built-in default neutral; (4) a **key map** of the
  route with the sheet's extent marked; (5) the **structures' elevations
  above the line, turned 90°**, a first-class template option with their
  own scale, offset, rotation and labels (number, type, height, the
  suspension strings' lengths); (6) **lines below the lowest conductor**
  per circuit or voltage, lowered by the ground clearance of the
  project's rules (by voltage; the rule's weather cases, else the largest
  sag drawn, said in the legend; a design code's ground preset fills them
  in through the existing rule machinery; no new engine calculation), on
  the sheets and in the sheet preview; options on/off, circuits, style,
  label, as well as or instead of the line above the ground.
- **Decided here** (not covered by the requests; the most conservative
  choice, open to the owner): where several ground rules check a circuit
  the line below its lowest conductor uses the **largest** distance (a
  **Conservative:** label in the theory page); a rule naming weather
  cases that are not drawn gives **no line** and a warning (rather than
  another case); low voltage has **no default limit** (without one every
  system with a voltage counts as high voltage); the plugin interface
  stays at **SDK 0.3** (the export is additive, as `rule-presets`: plugins
  built before load unchanged — the plan's 0.4 would have refused them
  under the 0.x rule); the listing is the command **`ohl
  sheet-templates`** (not `ohl drawing sheet-templates`, which clashes
  with its file argument); the corridor option is **`--corridor-width`**
  (`--corridor` is the LiDAR corridor); the band row `ground_clearance`
  stays empty until check results are given to it; overlap warnings of
  elevations count the line's own only (another version's elevation at
  the same structure overlaps by design); a version's wires and
  structures are those **within the line width** of the route.
- **Format and interface changes**: new file format `.ohlsheet`; new
  project library kind `sheet-templates`; the drawing settings
  (`app.drawings.<line>`) gain `sheet`, `side_offsets`, `plan_scale`,
  `logos`, `clearance_rule`, `markers` and `page = "template"` (older
  settings read with the defaults); new optional WIT export
  `sheet-templates` and manifest `[[sheet-template]]`; new DXF layers
  (`OHL_TITLE_FIELDS`, `OHL_TITLE_IMAGE`, `OHL_REVISIONS`, `OHL_BANDS`,
  `OHL_LEGEND`, `OHL_KEY_MAP`, `OHL_CORRIDOR`, `OHL_VERSIONS`,
  `OHL_CONDUCTOR_CLEARANCE`); DXF text codes `%%nnn` instead of `\U+`.

- **The swing arc in the vertical and radial clearances, and the windows
  explained as the vegetation panel** (owner's request): the wind of the
  Vertical and Radial clearances windows and `ohl zones` is none (the
  cases' own pressure), the **wind steps**, the **swing arc** or both,
  with the sides (`[wind] arc = true`, additive: files without it read
  and are written as before; `ohl zones --swing-arc`, `--wind-step`,
  `--wind-sides`; a preset may set it). The arc is the danger-tree
  check's (`positions::SwingArc`, not copied): between still air and the
  full blowouts of the sides chosen, the largest radius, rigid swing.
  **Vertical distance to the arc** defined as the height of the arc's
  lowest point over the point's plan position within the zone's width
  (its highest for a point above), found exactly at the place along the
  axis where the arc passes over the point (not the point's own
  projection on the axis); radially the arc's closed form; the ground
  against the wire swung every Δφ = 2 cm / R, each taken 2 cm thicker
  in a zone 2 cm wider (conservative). A finding on the arc names it and
  the angle (`ZoneFinding::arc`, CSV `arc_deg`). The windows: grouped
  settings with user-guide links, a line beside each value with a longer
  hover text, drawings of the zone and the wind positions, the shared
  wind choice (`check_help::wind_choice`, also in the vegetation panel,
  which is unchanged in behaviour) saying how many solves a windy case
  needs and suggesting the arc above 40; a progress bar with the time
  left; **Stop keeps the previous results**. Theory: [Vertical and radial
  clearances](../theory/clearances.md) (new page).
- **Speed of the vertical and radial clearance checks** (owner's
  request, with his data point: steps of 10 Pa on a long line did not
  finish in 10 minutes): measured, the time is the point and ground
  checks, not the solves (an invented line of 600 spans, 10 Pa steps,
131 solves, 10⁶ points: solves 68 s, vertical checks 91 s for the ground
and 103 s for the points, radial 264 s and 101 s; after: 65 s, 12 s,
17 s, 44 s, 15 s; the swing arc: 1.3 s of solves, vertical 5.9 s and
0.9 s). Made faster **with the
  same numbers bit for bit** (proved by tests against the earlier code,
  kept for them): one measure worked out per window, bounding boxes
  before exact distances, the highest ground per 5 m cell bounding the
  ground checks, the ground check and the points (in blocks, applied in
  order) on every core. Left out: warm-started solves (other numbers
  within the solver's tolerance), solving only sections near candidate
  points (needs the solve, or the rigid-swing simplification), the
  nearest line point instead of the piece over a point for the vertical
  (other numbers).
- **The vegetation and danger-tree panel** (owner's requests): **wind
  steps** for the danger trees — each windy case also solved in still air
  and at every pressure up to its own, by a step, from the sides chosen
  (the clearance windows' `WindSteps`), alongside or instead of the swing
  arc (new position kinds, added beside the others); the **required
  clearance and the allowance A as requirements by voltage** (one
  distance, a table, base + per kV; each wire with its system's voltage;
  single values work as before; the command line takes the rule file's
  forms); **every** grow-in and fall-in point kept and drawn, in two
  colours of the user's (kept with the view settings), one batch in 3D and
  one mark a pixel in plan and profile; **no line to the worst point**;
  the window a **dockable panel**; its **layers shown only while the panel
  is open** (closing it takes them off, reopening it or a saved run draws
  them again; docking or moving keeps them); runs **saved in the
  performed checks** (`ohl.vegetation`: settings, cases, per-span
  results, worst points, every point packed) and shown out of date when
  the line, the points or the ground changed since; every setting
  **explained** beside it with small drawings, grouped (wire positions,
  grow-in, fall-in, search) with links to the user guide; the **classes**
  chosen in a tick menu (default: by role); **design-code presets** of
  the check — a rule preset carrying a `[vegetation]` table (no change to
  the plugin interface; plugins built before are unaffected) — with its
  weather cases, those the project lacks created as one undoable change;
  **preset dropdowns** listing every enabled design-code plugin's presets
  grouped by plugin, in the rule editor, the clearance windows and the
  vegetation panel. Decided here (the requests left it open): a preset's
  weather case is matched **by name** (one of the same name with other
  values is kept as the project's, and the preview says so); a saved run
  tells the points changed when a point edit was logged after it (an undo
  counts: **conservative**); isoclearance lines are not saved with a run;
  plan and profile draw one mark a pixel (display only). ([user
  guide](../user-guide/analyses.md#the-vegetation-panel),
  [theory](../theory/lidar-analysis.md#wire-positions-checked),
  [plugins](plugins.md#vegetation-presets),
  [data model](data-model.md#performed-checks-ohl_sessionreports))
- **No tool in two menus** (owner): OHL-Design's tools live in the
  Tools menu and the inventory/hotbar, by theme (General, Alignments,
  Structures, Wires); the Structures menu no longer has Add structure…,
  New pylon from a template… nor Place on alignment points…. It keeps
  what is not a tool: the libraries, the tables, the display options,
  Spot structures (a calculation proposing a new line) and Angles around
  structures (a project setting). **Place on alignment points is Add
  structure on an alignment point**: the structure window, new on an
  alignment point, has **Also on other points of this alignment** (the
  former command's table: ticks, All/None, a row's own type), and the
  base on the ground or at the point's own height there; the structures
  ticked are added with it in one command (one undo), each tied to its
  point and turned along the line. Choices confirmed by the owner
  (2026-10-20): the other points take the window's extension
  and height adjustment with its type, a row's own type its base (the old
  command gave every one the base); nothing is ticked at first (the old
  command ticked every free point); the other points are always turned
  along the line; their routes and description are not copied from the
  window. **New pylon from a template…** is a
  command in the Tools menu's Structures group (and on the quick bar,
  key `new-pylon`), not a tool: it opens a window that builds a library
  type, with no click in a view. No saved quick bar or shortcut named
  the removed entries (they were never quick-bar commands); the tool's
  saved key `tool-add_structure` is unchanged. New pylon gets no
  keyboard shortcut (owner).
- **Danger-tree defaults = the utility practice** (owner): root-ball
  fraction k = 0.05, clearance allowance A = 2 m, a ground point within
  5 m of a tree base, a 50 m search distance (the half width of the
  corridor read) — in `VegetationParams::default`, `ohl lidar
  vegetation` and Checks › Vegetation alike; a design-code plugin's
  preset may give others, and each can be set (k = 0, A = 0,
  `--max-base-ground-distance inf` for no limit). **This changes the
  results of `ohl lidar vegetation` for runs that relied on the
  defaults** (before: 0, 0, no limit, 60 m; the application proposed
  k = 0.1 with no limit). The validation cases state their own
  parameters (results unchanged); tests of the check as before set them
  explicitly. Reading taken (not stated by the owner): the "search
  distance" is the corridor half width (`--corridor`, the window's
  "search ± m"). ([theory](../theory/lidar-analysis.md#danger-trees-2026-10-20),
  [user guide](../user-guide/analyses.md))
- **Unbalanced ice as a weather case; a weather case per span in the
  section solver** (owner: "the extra ice should just be like another
  weather case"; core change approved): the section solver takes a
  weather case per span (`ohl_line::solve::SpanCases`,
  `solve_section_span_cases`; `ohl_wirefe::State::span_states`): a
  span's own case sets its load per metre (weight, ice, the wind on its
  iced diameter), its temperature, the forces on the bodies on it and its
  surface radius; the section's case the strings' wind
  (**Simplification**, labelled), the jumpers, the core branch and the
  rolling of a section in the blocks (it rolls in the section's case; the
  spans' own cases act on it clipped in, as point loads do). One case for
  all spans is solved bit for bit as before (tested); the ruling span
  refuses spans in different cases. Validation `spancases-001…003`
  against an independent closed-form evaluation. The danger trees'
  unbalanced ice is rebuilt on it: the case's ice on the span (the other
  spans in the case without ice), more ice as a thickness (new) or as a
  load — **per wire (subconductor), as a weather case's ice load; before,
  the whole bundle's** (the heavier reading for bundles) — each a derived
  weather case (`WeatherCase::without_ice`, `with_more_ice_thickness`,
  `with_more_ice_load`); a concentrated load stays a point load. The
  wind now acts on the iced span's iced diameter (the old
  **Simplification**, wind on the bare diameter, is gone), and bodies on
  the span ice too. Example line, S1's second span: rime ice with wind,
  the case's ice on the span, blown 7.3 m across its axis (3.7 m before),
  its lowest point 0.67 m higher; glaze ice without wind 3 cm lower (the
  marker ball's ice). ([theory](../theory/wirefe.md#a-weather-case-per-span-2026-10-20))
- **Results out of date** (owner's decision for point edits, extended to
  line edits so the rule is the same): a result computed on demand keeps
  the revision it was computed from — a content hash of the line (with
  its weather cases, wire library and library files), of the point cloud
  (a sum of point hashes, changed by each point changed) and of the ground
  surface — and its window shows "Out of date: … changed since this run"
  with Run again when it differs; layers drawn from it are dimmed.
  Results are never dropped and never run again by themselves. Decided
  here (the task left it open): **an undo back to the state a result was
  computed from clears the mark**, since the revision is a hash of content
  and a new run would give the same result. Conservative choices: a
  change of any library file marks every result using the line (not only
  those reading that file); the wind side shown is a view setting and is
  not tracked (a galloping result follows the case shown as solved).
  ([user guide](../user-guide/desktop-app.md#results-out-of-date))
- **Box and lasso selection of LiDAR points** (owner's request): Alt +
  drag (box) and Alt + Shift + drag (lasso) with the selecting button in
  the 3D view and the profile select the points drawn inside (the rule of
  a click); their class is changed together as one `change_points`
  command, with a class filter; coordinates only for a single point. A
  command takes at most 10⁶ points (more: refused with the reason); from
  100 points it is written packed (zstd + base64 of its columns, some 10
  bytes a point) — a change of the log's format, older lists still read.
  The point store keeps a block's points in runs while changing them
  (stored bytes unchanged). ([user guide](../user-guide/desktop-app.md#editing),
  [data model](data-model.md#commands-the-only-way-to-change-a-project))
- **LiDAR points: what can be clicked, and changing a point** (owner's
  request): clicking LiDAR points was unreliable because a click looked at
  the points loaded when the project opened (a sample thinned to
  `--points`), not at those drawn — a streamed survey draws the nodes of
  its point budget, the chunks of points in memory draw a share at levels
  1 and 2, the plan draws none, the profile's dots were not clickable at
  all. Now **every point drawn can be clicked and snapped to, and no
  other** (3D: the nodes or chunks drawn in the last frame; profile: its
  dots; plan: none); of points within a pixel the one nearest the eye.
  Picking is display only: nothing computed changes. The Edit tool on a
  LiDAR point opens a window to change its **coordinates and class**: the
  new session command `change_points` changes the stored cloud point by
  point (blocks, levels of detail, bounds and counts made again from the
  blocks, the other points kept in order), logged and undone bit for bit
  as one step, through the server like every edit; the clearances are
  checked again and the ground built from the cloud when the project
  opened is built again. Conservative choices left to the owner: the
  clouds belong to the project, so a change is logged and undone on the
  project line it was made on and revisions of a line do not hold the
  points; several points at once only by the command (views select one
  point; box or lasso selection left for later); an empty block made again
  by an undo gets a new row id (its content is bit for bit the same).
  ([user guide](../user-guide/desktop-app.md#editing),
  [project files](../user-guide/project.md#changing-points),
  [data model](data-model.md#commands-the-only-way-to-change-a-project))
- **Rules by voltage and rule presets (confirmed by the owner):** a rule
  file is written only when every rule is complete (an unfinished rule
  shows "not written yet"); **Load preset…** reads the design-code plugins
  with the parameter file of Criteria › Design code, or none. A design-code
  plugin's presets for a national code (kept outside the repository) check
  the direct distances at the largest sag and under wind and the vertical
  distances at the largest sag; building distances are screened out to
  the largest horizontal distance the code asks; crossing presets that
  depend on the distance to the nearer tower or support are given only
  when that distance is entered (nothing assumed); its LiDAR class mapping
  confirmed.
- **Clearance rules by voltage in the app, rule presets from plugins**
  (owner's request): wherever a rule's distance is set (the Clearances
  tab's new rule editor, the Vertical / Radial clearances windows and
  their Add a rule) it is one distance, a table "up to kV: distance" or a
  base plus an amount per kV, with a preview at the project's voltages
  and 0 kV — as the rule files already allowed. The windows now keep a
  rule's requirement as written (`ZoneRule::distance` is a
  `Requirement`): a rule by voltage is one row, each wire checked with
  its voltage's distance, where an import gave one row per line voltage
  before (results unchanged: the same distance per wire); a table that
  does not reach a wire's voltage stops the window's check and `ohl zones`
  with an error, as `ohl line` already did. Design-code plugins may give
  presets of any clearance rule set through the new optional
  `rule-presets` export (interface 0.3 unchanged: the host looks for it,
  plugins built before load as they are); the app merges a preset into
  the chosen rule file or replaces its rules, one command; the CLI writes
  them (`ohl code rule-presets`, `ohl code generate`).
  ([user guide](../user-guide/desktop-app.md#distances-by-voltage),
  [plugins](plugins.md#rule-presets))
- **Danger trees built** (in active development, not approved): the
  method below as written, with these changes and readings
  ([theory](../theory/lidar-analysis.md#danger-trees-2026-10-20)):
  within the fall sector the distance is \(|q - H| - L\) (negative
  inside: by how much the tree reaches past the wire) instead of
  \(\max(0, \ldots)\) — same verdict, the margins as before; the
  resting angle at every TIN edge crossing along the fall direction (the
  TIN's resolution, exactly), rising ground seen from the base
  (**Conservative**); the smallest over a wire as the distance between
  the wire and the hinge circle (exact per piece; on a swing arc within
  1 mm below, **Conservative**); a tree whose nearest wire point is
  outside the sector's angles (sheltered) searched along the wire by the
  bound of a distance (**Simplification**), no shelter credited on an arc
  (**Conservative**); the arc's angles from the smallest to the largest
  of the three, the bundle's outer envelope on the arc (both
  **Conservative**), the three wires' points interpolated at the point's
  place (no radius interpolation error); the case's ice on one span as a
  weight spread along it, the wind on its bare diameter
  (**Simplification**); isoclearance slopes above 5:1 taken as 5:1 in
  the bounds (**Simplification**). Root-ball fraction: default 0 in the
  command line (results bit for bit as before), 0.1 proposed in the
  application (a user setting; open point for the owner). `ohl lidar
  vegetation` solves all the cases chosen together and reports per span
  the worst over them with case and position; its CSV changes to one row
  per kept point with case and position (`DANGER_CSV_HEADER`; the
  per-case blocks are gone). The vegetation, galloping, right-of-way and
  as-built commands now read a project file's own libraries, as `ohl
  line` does. New drawing layer `OHL_ISOCLEARANCE`. The cross-section
  view is left for later (no such view in the application yet).
- **Tree base needs a ground point near it** (owner, via the
  coordinator): `max_base_ground_distance` — a vegetation point's ground
  height (its tree base) is taken only when a ground point the surface is
  built from lies within that horizontal distance, else no base, no
  fall-in check, counted as "no ground point within the distance".
  Default no limit (results unchanged); a design-code plugin's preset
  may give 5 m (the private national-code plugin with root ball 0.05, a
  2 m clearance allowance and a 50 m search distance).
- **Danger trees (method written, to be built):** fall-in about a root
  ball (hinge at a fraction k of the height), the tree falling towards
  each wire point and resting on the ground along its direction; grow-in
  with the wire hot, blown left and right, and along its swing arc (a
  rigid swing about the attachment points' axis, the largest radius);
  an optional unbalanced-ice case per span; isoclearance lines over the
  corridor for one case or the worst. ([theory](../theory/lidar-analysis.md#danger-trees-2026-10-20))
- **Licence:** OpenOHL is under the **AGPL-3.0-or-later** (forks and
  modified versions, also those offered as a network service, stay open
  under the same licence). The plugin interface (`crates/ohl-plugins/wit/`)
  and everything under `plugins/` stay under **MPL-2.0**, so plugins may
  keep any licence. ([ADR-0011](../adr/0011-license-agpl.md))
- **Comparison tool for the existing line** (local): the "ice as weight
  only" variant no longer shows the wind on the bare diameter since an ice
  load without a density is taken as glaze ice (2026-10-19).

## 2026-10-19

**Calculations**

- **Tension-only members** carry compression up to their own elastic
  buckling load π²EI/L_k² (L_k = k·L, k = 1 unless given), then stay at
  it ("bowed"); always solves. Not modelled as sagging cables.
  ([theory](../theory/structures-fe.md)) Usage of bowed members without
  the Euler term, and k depending on the joint type: open point O12.
- **Tension-only members that bend** (`bending = true`): act as a frame
  while in the model, as a truss when bowed or slack; members can be
  solved without a check (`check = false`). Read from another program's
  "tension-only beam" for the imported towers; the reading confirmed by
  the owner. The structure exports keep the bending.
- **Relaxed refusal accepted:** a truss rod may carry load across itself
  through a relative hold (flagged "LARGE", not refused). Rebuilding an
  imported tower's rounded primary joints: later.
- **Joint stabilisation:** a free joint on a straight line of members is
  held relative to the line's ends (its load goes through the structure;
  with P-Delta it follows its face); others are held to the ground. A
  relative hold above the old 1 % limit is flagged ("LARGE"), no longer
  refused; a hold to the ground above it is still refused.
- **Clearances to the conductor's surface**, not its axis (the iced
  radius in a case with ice), less the polyline's error bound; sampling
  errors bounded on the safe side; galloping ellipses as circumscribed
  polygons. Results move by the radius (about 17 mm bare, 30 mm iced).
- **Phase distance (check plugins): between the conductors' axes**, as
  the codes define it (LeV: between axes), less the polylines' bounds.
  ([check](phase-distance-check.md))
- **Ice given as a load without a density:** taken as glaze ice
  (913 kg/m³), never the bare diameter.
- **Allowable spans between two line angles:** interpolated linearly in
  sin(θ/2), on the safe side for a structure responding linearly.
- **Wind on structures:** a load case may give the structures their own
  wind pressure and factor, apart from the wires' (`[case.structure]`,
  `.ohllc` 2); one rule for the line and OHL-Structure.
- **Drag and shielding of lattice towers:** factors are data (a `[wind]`
  table, from a design-code plugin or typed), never built in; without
  them the generic drag stays and nothing is shielded. Only members in a
  face turned to the wind are shielded; the face outline is the convex
  hull of its joints; no factor outside a table's range.
- **Wind bands on an extension** rise with the structure (heights above
  the ground of the structure as written); the force is unchanged.
- **Transparency:** every conservative assumption is marked
  **Conservative:**, every other simplification **Simplification:**, in
  the theory pages ([conventions](../theory/conventions.md)).

**Formats**

- **Structure exports keep a tension member's bending** (owner): a
  `tension` member with `bending = true` is exported as a frame
  (IFC4 analysis view: `.RIGID_JOINED_MEMBER.` with rigid ends, as frame
  members are; SAF: `Behaviour in analysis` Standard), its tension-only
  behaviour kept as information (IFC `OHL_Member` Kind `tension`,
  `Bending` true; the analysis member's `Description`; SAF `Layer`
  "OHL: tension only, bending"; CSV `bending`). Members with
  `check = false` are exported as any other, flagged (`OHL_Member`
  `Check` false, `Description` / `Layer` "not checked", CSV `check`).
  SAF's "Tension only" is not used for them: it carries axial force only.
  ([exports](structure-exports.md))
- `.ohls` version 6: body-generator options, recipe values of the
  structures' wind, bending/unchecked members (one unreleased raise).
- `.ohllc` version 2 (structures' wind), `.ohlfe` version 3, the section
  library keeps version 1 with an optional `[wind]` table.
- Drawing settings are stored in the project, per project line.

**Applications**

- Drawings: one **line width** (±40 m) for the profile and the plan; it
  decides what is drawn (wires, structures, survey and LiDAR points).
  Sheets end only at a structure on the line or at an alignment point
  without a structure, never beside the line; the break clearance is a
  setting (2 mm on paper, 0 to 20 mm); a sheet preview in the app; the
  structures' elevations may be drawn above or below the line on the
  plan (90°, offset from the centreline).
- Level 2 and 3 members and poles drawn with their real outline; solids
  within a distance of their own (400 m by default).
- Drag handles in the line's 3D view; joints stay editable in
  OHL-Structure only; a dragged point takes what hangs below it along.
- Tools grouped by theme in the inventory and the Tools menu; an option
  for tools to act on left-click; a searchable coordinate-system picker.
- OHL-Structure: results on the chosen extension, the pole check, the
  structure report (format to define with the owner); `ohl structure`
  asks OHL-Structure's headless tool, the programs exchanging JSON.

**Roadmap answers:** see ROADMAP.md Part 1 (O1–O13) for the owner's
answers in full.
