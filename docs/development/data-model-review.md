# Data model review (2026-10-17)

A review of the data the program keeps — the line model (`ols-line`),
the project file (`ols-session`), OLS XML and the library formats — after
the points-and-objects work. It says what holds up, what is broken, and
what is worth changing, most urgent first. §1, §2 and §4 are done
(marked ✓; how, in [Done](#done)); §3 is open. The [data model
map](data-model.md) describes the model as it is.

## What holds up

- **One line model, many stores.** The line file, OLS XML and the
  project rows carry the same `ols_line::Line`; OLS XML destructures every
  struct, so a new field cannot be forgotten silently, and it round-trips
  everything (spacers, taps, span and jumper points, objects, spreads,
  defaults, solver options, plugin values).
- **Every change goes through commands**, validated on the whole line,
  logged with its inverse, undoable; deletions list what goes with them.
- **Old files keep opening**: serde conversions (`SectionFile`,
  `StructureFile`, v1 attachments, v2/v3 routes) upgrade them on reading.
- **Structures as explicit graphs** (points, strings, wire points with
  sets) and **points with one object** (body or clamp) are general enough
  for what is planned (V + I, TAKs, taps, spacers).
- **Library ids are the only cross-file references**; units are SI
  inside, and the formats are versioned with a header.

## 1. Broken today (small fixes, no change to the model) — fixed

All seven fixed on 2026-10-17 (tests in `ols-line` and `ols-session`).
Opening a project now drops stale circuit starts; a project or line file
with a span point's distance beyond its span, repeated spread labels or
impossible stringing values is refused on opening (none were found in the
lines checked). The `Live` fix marks every section when the line settings
the solve reads change, and `Live::set_library` solves again with an
edited cable library.

| # | Where | What happens | Fix |
|---|---|---|---|
| 1.1 | session: deleting a **structure** or a **set** | jumpers on taps of the sections it deletes are not listed (`structure_deletion`, `attachment_deletion` do not call `jumpers_hanging`): the user confirms, then the command is refused with a validation error | list them as a section deletion does |
| 1.2 | app: **section editor** | changing a section's structures keeps `span_points` with their span **index**: a structure added before them moves every span point (with its object, spacers, taps) one span on; a removed one can push them off the end (refused) | shift the indices with the points; refuse or ask when a removed span holds points |
| 1.3 | session: `circuit_starts` | not renamed with a structure or a set, not removed with them, never validated: a stale start stays and comes back if the id is used again | follow renames and deletions; validate |
| 1.4 | `Defaults.jumper_clamp_length` | allowed up to 100 m, while a clamp is refused above 1 m: a default over 1 m makes clamps the line then refuses | range 0–1 m |
| 1.5 | session: opening a **newer** project file | tables are created and migrations run before the newer schema is refused: an older build changes a newer file | check the schema first |
| 1.6 | `Live` (CLI project, server): line settings changed | solver options, defaults, bimetallic model, creep cases, systems mark no section to solve again; library edits neither | mark all sections when `meta` changes |
| 1.7 | validation gaps | spread labels not checked unique; a span point's station or distance not checked within its span; stringing tension / temperature and unstressed lengths not checked positive and finite | add the checks |

## 2. Inconsistencies in the model (format changes — to decide)

These change the line file (a new version, older files still read).

| # | What | Why it matters | Proposal |
|---|---|---|---|
| 2.1 ✓ | **Jumper ends**: `from`/`to` are required even when `from_tap`/`to_tap` replace them (an empty placeholder) | code that reads `from` (clamp structure default, bundle and colour from the section through `from`, deletion filters) must remember the tap; easy to get wrong | one `JumperEnd` enum per end: a wire point (`{structure, set}`) **or** a tap |
| 2.2 ✓ | **Spans by index** (`SpanPoint.span`, `Stringing::Sag.span`, `UnstressedLengths.lengths`) | the only list-position references in the model (see 1.2); merges of two branches touching a section's structures can mis-place points | name a span by its back structure (`span: "T4"`) or keep the index but re-index in one place (`Section::insert_point` / `remove_point`) used by every editor |
| 2.3 ✓ | **Three ways to support a jumper**: `support` (one string by coordinates), `supports` (clamps on structure strings), jumper points with clamp objects; positions as `fraction`, `at` (0–1) and `JumperPlace` | three placements, three solver paths | supports become jumper points (a `support` clamp object naming its wire point); `support` read as one |
| 2.4 ✓ | **Places with priorities**: `SpanPlace` (station › distance › fraction) and `JumperPlace` (distance › length › fraction) keep unused values and differ in order | a stale fraction is written and read back; the two orders confuse | an enum per place (`{fraction}` \| `{distance}` \| `{station}`) |
| 2.5 ✓ | **Spacer ends** need not hold a clamp object (decided: they do); taps must | two rules for the same kind of point | require the clamp at spacer ends (the app already puts one), or say clearly it is optional |
| 2.6 ✓ | **Point labels cannot be renamed** (spacers, spreads, taps use them) | a user cannot tidy labels | a `rename_point` command that follows spacers, spreads and taps |
| 2.7 ✓ | **Legacy types still public**: `SpanLoad` (with its own `forces`, a copy of `LineObject::forces`), `SpreadLoad` (also the solver's working form), `Line.alignment`, `Route.legacy_*` | two load formulas to keep equal; legacy fields reach `validate` | keep them in the `*File` reading types only; the solver reads `SpreadObject` |
| 2.8 ✓ | **Defaults as positional arrays** (`DEFAULT_THERMAL`, `DEFAULT_CLEARANCE` indexed by number in `ols-thermal`, `ols-clearance`); a few literals (insulator EA 5·10⁷ in `ols-structures`, the clamp length 0.1 used by serde while the app uses the project default) | a reordered array changes results silently | named constants or small structs; one place per default |
| 2.9 ✓ | **`Display.phase_colors` keyed by phase label** only | `L1` of two systems always the same colour | keep (by system colours exist), or key by system + label |
| 2.10 ✓ | **`Section.circuit`** (free text) beside the derived circuits | two meanings of "circuit" | rename to `circuit_name` / a description, or drop |
| 2.11 ✓ | **`Calculation`** lists condition × method by hand (kebab-case, the only one) | each new condition or method is many edits | `{condition, method}`; read the old names |
| 2.12 ✓ | **Line file version** not raised for points and objects (still 4) | an older program refuses a new file with "unknown field" rather than "newer version" | raise to 5 with the next format change |

## 3. The project file

| # | What | Consequence | Proposal |
|---|---|---|---|
| 3.1 | Span points, spreads and jumper points live **inside** their section / jumper row; structures hold their strings and wire points | undo is refused when another user changed the same section after; branch merges conflict on the whole list | fine for now (one designer per section); if parallel editing grows: merge lists by label, or make points their own rows |
| 3.2 | **`meta` is one row** (name, crs, display, systems, creep cases, bimetallic, solver, defaults, circuit starts); puts carry no base version | two users editing different settings overwrite each other; undoing one setting is blocked by a later other one | split into rows (`systems`, `solver`, `defaults`, …) or check `version` on put |
| 3.3 | **Library files** are shared by all project lines but versioned per branch; not logged, not undoable | editing a library on a phase line changes it for `main`; undo cannot bring it back | a `line` column, or log library edits as commands |
| 3.4 | Clearance rules, galloping, thermal ambient, costs, poles, structure models are **files beside the project** | a project copied or bundled alone loses them; not versioned | store them as `library_file` kinds as the others |
| 3.5 | Rename = put under a new id + delete | history and report fingerprints see a removal and an addition | acceptable; a stable internal id only if history per object is wanted |
| 3.6 | `replace_line` ignores the order of the replacing line | structure / section order not reproduced | pass `ord` |
| 3.7 | Spacers, span points and jumper taps not in the spatial index | not found by area queries (server, web) | index spacers; jumpers' bounds with their taps |
| 3.8 | Migrations keyed by probes and flags, not by the schema number; `line_snapshot` still created | harder to reason about | one migration per schema step |

## 4. Library and exchange formats

| # | What | Proposal |
|---|---|---|
| 4.1 ✓ | **Units**: the convention says library values carry their unit (`"0.6 m"`), but `.olsload` takes plain numbers only, `.olsg` `tilt` and `centre_above_rest`, `.olscost` prices and some kV keys too; `.olspd` (plugin) has its own parser taking one unit | make `.olsload` fields unit-aware (plain numbers still read as SI): the most visible gap, a new format |
| 4.2 ✓ | **Headers**: 13 formats use `check_header`, six their own copy (cables, pylon templates, the line file, `.olspd`, check reports, colour files) with different rules (`format` optional or required, older versions accepted or refused, bundle exactly 1) | one policy: header optional, any version up to the current read, newer refused |
| 4.3 ✓ | **Typos**: unknown keys refused in cables, rules, galloping, classes, `.olsload`, load cases, models; silently ignored in weather, thermal ambient, structures, insulators, poles, pylon templates | refuse everywhere (warn first for a release) |
| 4.4 ✓ | **Names**: format ids singular and plural (`ols-wire` holds `[[cable]]`, `ols-weather-cases`), constants named three ways, extensions as literals in ~30 places (no constant for `.olslc`, `.olspd`) | constants in each crate, used everywhere; ids kept (they are in files) |
| 4.5 ✓ | **Shipped libraries**: the app looks for `libraries/structures`, `libraries/insulators`, `libraries/clearance-rules`, which do not exist; an empty `libraries/phase-distance` is left over | ship small synthetic examples or drop the lookups |
| 4.6 ✓ | **Examples and schemas lag**: the synthetic line files are version 2; the XSD namespace says 1 while its `version` is the line file's (4); `.olsfe` has no example; JSON Schema only for `.olsw` | rewrite the examples at the current version; say in the XSD that `version` follows the line file |

## Done

On 2026-10-17, each step its own commit, the line file now version 5:

- **§1** all seven fixed, as proposed.
- **2.1** `JumperEnd` (a wire point or a tap) for `from` / `to`; version 4
  `from_tap` / `to_tap` read.
- **2.2** spans stay referenced by index, moved in one place:
  `Section::set_points` (span points, the sagged span, clipped lengths),
  used by the section editor. Naming spans by their back structure was
  left: it breaks when a structure is put in a span the same way, and the
  index stays the simplest form for files.
- **2.3** supports are jumper points holding a clamp with a `hold` (a
  structure's strings, or a string from a point); `Jumper::supports()` /
  `support()` give the solver what it took before (results unchanged).
  The solver still has its two paths (a string from a point, a structure's
  strings): the placement is one, the mechanics differ.
- **2.4** `SpanPlace` / `JumperPlace` are enums; several keys in older
  files read by the old priority.
- **2.5** a spacer's ends must hold a clamp; bare ends of older files get
  a clamp of no weight when read.
- **2.6** `rename_point` (session) / `Line::relabel_point`; the Span
  points table relabels with it.
- **2.7** `SpanLoad` / `SpreadLoad` moved to `ols_line::legacy` (read
  only); the working form is `objects::SpreadStretch`. `Line.alignment`
  and `Route.legacy_*` stay where they are: turning them into routes needs
  the whole line (`Line::upgrade_routes`), not one route's file form.
- **2.8** `Defaults::PROGRAM` by name; one insulator EA constant.
- **2.9** kept: colours by phase label, by system through the system's
  own colour.
- **2.10** `Section.circuit` → `circuit_name` (`circuit` still read).
- **2.11** `Calculation { condition, method }`, written as before.
- **2.12** version 5.

Then §4:

- **4.1** `ols_core::units::si` reads plain `f64` fields from a number
  (SI) or text with a unit; `.olsload` takes units (its templates are
  written so), a galloping `tilt` angle text. Prices and `_kv` keys
  keep their unit in the file's currency or the key's name.
- **4.2** every header through `check_header` (both optional, newer
  refused), the six copies gone.
- **4.3** a mistyped key refused everywhere — `.olsload` and objects on
  points were the ones letting typos through (a flattened shape); a test
  checks every library format.
- **4.4** `*_FILE_FORMAT` / `*_FILE_VERSION` / `*_FILE_EXTENSION` in
  each crate, used for every extension.
- **4.5** example insulators shipped (`libraries/insulators`); the
  structure-type and clearance-rule folders no longer looked for (they
  belong to a project and to a design code).
- **4.6** the example lines written at version 5 (their version 2 form
  kept as test data, read to the same line); an `.olsfe` example; the
  XSD says its `version` is the line file's. A JSON Schema stays for
  `.olsw` only.

Found on the way and fixed: turning a jumper along its circuit now turns
all its points' places (only its supports were turned, and a tap end
stayed at its old end); the clearance check took every clamp on a span as
a tap (a spacer's clamp would have hidden the wire near it).

## Suggested order

1. ~~The fixes of §1~~ (done).
2. ~~§2~~ (done, line file version 5).
3. ~~§4~~ (done).
4. §3.2–3.4 when several people work on one project at once.
