# Interphase spacers: a string between two conductors in a span (proposal)

Status: **decided, being built.** Done: the data (span points, point
loads by fraction or distance, spread loads, spacers, the project's
default values with their file), checked (a spacer's sections by finite
elements only), saved, logged, in OLS XML; spread loads and span points
in the solve; places by station; **the spacers in the solve**
(`solve::solve_spacer_group`, below); **the app**: the Add on span tool
(span points, point loads, spread loads where the span is clicked; by
fraction, distance, station or regular spacing), spacers with Add string
between two span points, both selectable, drawn, in Properties and in the
tables (Span points and loads, Spacers), deleted with what goes with
them; Criteria › Default values with its `.olsdef` file; the
distance between conductors moved to a check plugin, with a rule for
conductors joined by spacers ([the core's
part](phase-distance-check.md)). To come: the sweep of
the other default values.

Decided with the user: spans with spacers refuse the ruling span; span
points (and point and spread loads) are placed with one span tool — by a
fraction of the span, a length along it, or a regular spacing — and the
spacer is drawn with the add-string tool between two span points; a
spacer's compression limit defaults to 10 000 N, from the project's
default values (Criteria › Default values, exported and imported as a
file); spread loads by start, end and N/m; the LeV check moves to the
plugin with the phase-spacer rule, the core keeping what it needs to
run (and compared for speed).

Added by the user: a place on a span may also be given by its **station**
(along the span's alignment, between its two structures' stations);
with the span tool the object is placed where the span is clicked. At
the end, a sweep of the program's **non-plugin default values** (e.g. the
jumper exit angle, insulator stiffness, label distance) into the
project's Default values and their file.

### How the spacers are solved (as built)

The sections joined by spacers (`solve::spacer_groups`) are solved in
turn, not in one model (so every model feature — stress-strain curves,
bimetallic cables, rolling in the blocks, the jumpers' pull — keeps
working): the section at a spacer's end `a` **holds** it, as a bar from
its span point to a fixed point where the section at end `b` puts its
span point; the section at `b` is **pulled** there by the bar's force.
Per pass: the pulled sections under the current forces f, then the
holding ones at the new positions, giving the bars' forces g(f); f is
moved towards g with **Aitken's relaxation** (the step from the last
two residuals) until they differ by less than 1 N (or 1e-4 of the
force), at most 40 passes (two-sided simple relaxation oscillates: a
stiff bar between two soft wires). Each end carries half the spacer's
weight and the wind on it (across its span, from the case's side); a
bundle's model takes its share. A spacer without a length takes the
distance between its ends in still air at the sections' stringing
temperatures. Reported on the holding section (`SectionShape::spacers`):
its ends, its axial force (tension positive), slack (tension only,
pushed), over its compression limit (flagged). Also in the incremental
solve and in each pass of the jumpers' pull. Not yet: spacers on a
section that shares an assembly with another (a TAK) — refused with an
error.

Original proposal: What it would take to model an
insulator string held between two conductors in a span (an interphase
spacer, *Phasenabstandhalter*), used to keep conductors apart (against
galloping, conductors swinging together, or to reduce the distance a
line needs). Written to be discussed before anything is changed; every
point below is an engine change.

## What it is

A rigid or tension-only insulator (usually a composite rod that takes
compression) clamped on two conductors of the same span — two phases,
or a phase and an earth wire — at a place along each, often several in
one span. Unlike today's strings it hangs from no structure: both its
ends move with the wires.

## Data (line file, project, OLS XML)

A new object of the line, `spacers`:

- `id`, `description`;
- its two ends: each a **section, a span and a place along it** (m of
  unstressed length from the span's start, or a fraction) — the same
  place idea as a point load (`loads`) or a jumper's helping clamp;
- the insulator: a library label (`.olsi`: length, weight, wind area,
  axial stiffness) or its own values; whether it takes compression and
  up to what force (as `compression` on strings); a clamp mass on each
  conductor;
- validation: both ends on spans of FE sections, the two places within
  their spans, its length near the still-air distance between them (or
  given and checked).

Session: a new object kind (logged, undoable, kept with the branch);
deleting a section lists and deletes its spacers (deletion impact);
renaming follows. OLS XML element and XSD; data model page.

## Solver

Today each section is its own finite-element model; groups sharing an
assembly (TAKs) are solved together by passes (Dirichlet–Neumann). A
spacer joins two sections in the middle of a span, stiffly:

1. **Nodes on the wires at the spacer's places.** The section's span is
   cut there (as for point loads now): a node at the given unstressed
   length, so the clamp stays at the same place on the wire.
2. **One model for the sections joined by spacers.** The spacer is a
   bar between a node of each section's cable (both ways when it takes
   compression, tension-only otherwise; `ols_wirefe` already has both
   kinds of truss). Joined sections are solved **in one model** rather
   than by passes: a stiff bar between two wires would make the passes
   converge slowly. `coupled_groups` is extended to sections joined by
   spacers (with the TAK groups).
3. **Loads:** its weight (half at each end) and the wind on it, across
   the line from the case's side (as on strings and jumpers).
4. **Stringing reference:** spacers are fitted after stringing, so the
   unstressed lengths are cut without them (as jumpers are); in the
   weather cases they act.
5. **Ruling span:** cannot represent a spacer (one tension per section,
   spans independent): such sections are refused with RS, or the spacer
   is left out with a note — to decide.
6. **Outputs:** the spacer's ends (positions), its force (compression
   or tension), flagged beyond its limit; the sections' shapes as now.

Expected cost: the sections joined by spacers solved together (a model
the size of both), once per weather case instead of separately — about
the cost of solving them each, plus a little.

## Checks and other parts

- **Clearances, wind clearances:** unchanged (they use the solved
  wires); the spacer itself could be drawn and given a clearance to the
  structure.
- **LeV distance between conductors:** Annex 6, 2.2.4 gives a rule for
  conductors with phase spacers (with d_f, the largest sag between
  spacers) — the check would take the stretches between spacers.
- **Structure loads:** unchanged (through the conductors' tensions).
- **Galloping:** the ellipses could take the spacers into account
  (the code's own method) — later.
- **Import:** PLS-CADD files that hold spacers, if they do, mapped.

## App

An **Add spacer** tool (click a conductor, then another in the same
span, at the places; snapped along them), a **Spacers** table (id,
section/span/place of each end, insulator, compression, force in the
case shown), properties, drawn in the 3D, plan and profile views,
selectable, deletable (listing what goes with it).

## Order of work (when decided)

1. Data, session, files, validation (tests on the synthetic line).
2. Nodes at places along spans; one model for joined sections; the
   spacer bar; outputs (tests: two level wires pulled together by a
   short spacer, forces by hand; a spacer in compression under wind).
3. App tool, table, drawing.
4. LeV 2.2.4 in the check; docs.

Questions to settle first: compression limit by default; RS sections
refused or spacer ignored; places given as length or fraction; whether
spacers between more than two conductors (a "star" spacer) are needed.
