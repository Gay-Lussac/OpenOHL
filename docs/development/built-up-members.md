# Built-up members (plan)

*Plan, 2026-11-02 — not approved. Asked by the owner: "plan
the double star and 4 corners to be shown or at least calculable"; "Go
with A". Stages B1 and B2 built (in active development, not approved
yet); the others not built. Open point O20 in the [roadmap](../roadmap.md);
comment there or under each stage.*

## What there is today

A member made of several profiles bolted together is common in lattice
towers:

| Arrangement | What it is | A tower program calls it |
|---|---|---|
| **Two angles back to back** | two angles, their long (or short) legs side by side with a gap (a filler plate) | double angle, back to back |
| **Two angles in a star** | two angles heel to heel, turned 180°: a cross of four legs | double star, double cruciform |
| **Four angles in a cross** | four angles heel to heel round a point: four double legs | cruciform, four angles |

Today such a member is a `general` cross-section of typed values (area,
second moments, moduli), so it **is calculated** — the finite-element
model takes the whole as one bar — but:

- it is **drawn as a box** of its outline (the 3D view, the joint
  close-up and face view);
- its properties are **typed**, not worked out from its parts (an
  import takes the tower program's);
- the member check knows only how many profiles lie side by side (the
  design data's `plies`, for the bolts' bearing): **not** the rules for
  built-up members — each profile buckling on its own between the
  stitch bolts that tie them, the connectors' spacing;
- the joints see one profile: the bolts through every ply, the filler
  plates and which leg of which angle bolts where are not known.

The tower library of a real line (kept locally) holds 48 cross-sections
of two angles back to back, 14 of two angles in a star and 8 of four
angles in a cross, all brought in as `general`.

## Stages

Sizes as for the joint details: S a few days, M one to two weeks.

### B1 — Built-up cross-sections in the library (M): calculable

- New shapes in the cross-section library (`ohl-sections`):
  `double_angle` (two angles back to back: legs, thickness, root
  radius, the gap; long or short legs together), `star_angle` (two
  angles in a star: legs, thickness, the gap) and `cross_angle` (four
  angles in a cross: legs, thickness, the gap). Each made of its parts:
  the area, the second moments about the section's axes and its
  principal axes (a star's lie at 45°), the torsion constant (the sum of
  the parts'), the elastic and plastic moduli, the wind width, the
  weight — **worked out**, with the parts' own properties kept (each
  angle's radius of gyration about its minor axis, for B4).
- The typed properties of a `general` section stay accepted; a
  built-up shape is preferred.
- Validation: against hand calculations (a validation case per shape),
  and locally against the tower program's values of the real line's 70
  built-up sections (the differences said; a root radius or a gap
  missing explains some).

**Built (2026-11-02, in active development, not approved yet).** What
was done:

- `double_angle` (`h`, `b` — left out: equal legs —, `t`, `r`, `gap`,
  `back = "long"` or `"short"` for unequal legs), `star_angle` (`h`, `b`,
  `t`, `r`, `gap`; unequal legs allowed: the legs `h` along ±z, `b` along
  ±y) and `cross_angle` (`h`, `t`, `r`, `gap`; equal legs) in
  `ohl-sections`. Each is drawn from its angles, root radii included, so
  the area, centroid, second moments, principal axes, elastic and plastic
  moduli and wind width come out of the same integrals as for every other
  shape. Torsion: the sum of the angles' Σ b t³/3; shear areas: the sum of
  the angles'.
- **The gap is required** (question 1 below still open): a missing or
  negative gap is refused, naming it.
- Each angle's own properties: `Section::part_properties()` (its minor
  radius of gyration, shown in OHL-Structure's cross-section form as
  "iv, one angle").
- Typed in OHL-Structure (Cross-section library form); drawn for now as
  the outer edge with the gap filled (3D view, joint close-up); IFC: by
  name with the values (no parametric profile); SAF: a general section of
  that edge; CSV and the plugins' member data: the dimensions with `gap`.
- Validation: unit tests by the parallel-axis theorem (equal and unequal
  legs, root radii), cases `sections-001…003` (calculation
  `sections.properties`, `benchmarks/structures-fe/built-up-sections.toml`
  with `reference_built_up.py`); locally, the real line's 70 sections: see
  [the decision log](decisions.md#2026-11-02) — with each section's gap
  fitted, the second moments within 0.3 % (crosses) and mostly 1–3 %
  (stars, back to back: its single angles differ as much); the gaps vary
  from 0 to about 17 mm, so no default would fit.

### B2 — Importing them (S)

The local importer of the tower program's files maps its built-up
angles to the new shapes (the arrangement and the gap from its data,
when it gives them), keeping its values in the description to compare.

**Built (2026-11-02, in active development, not approved yet).** The
local importer of a tower program's export (a private plugin, not in
this repository) maps two angles back to back to `double_angle` (the
long legs back to back), two in a star to `star_angle` and four in a
cross to `cross_angle`, with OpenOHL's own formulas (`ohl-sections`).
The export gives neither the gap nor the root radius, so:

- the **root radius** is taken from the export's area (A / n =
  t (h + b − t) + (1 − π/4) r² for n angles): the area matches by
  construction — an assumption: the material the export's area holds
  beyond the plain legs is put at the root;
- the **gap** is fitted so that the computed iu + iv (the same in any
  axes) is the export's, to 0.1 mm, between 0 and 50 mm;
- the section is mapped only when, with that gap, the computed iu and iv
  are **each within 5 %** of the export's (the local comparison of B1
  found up to about 4 % for sections that are what their label says:
  the single angles differ as much); else — and for a section labelled
  back to back that carries a star's values (iy = iz with iu ≠ iv) — it
  stays `general` with the export's values, and its description and the
  import's notes say why;
- the export's values (area, iy, iz, iu, iv) and the computed ones with
  their differences are kept in each section's description; the
  properties used are OpenOHL's, so a member's stiffness and its wind
  width (the outline: two legs and the gap across a pair, where the
  export gives one leg) change a little against an import as `general`.

Double channels and the other profiles stay as before. A first trial on
a real line's export: see the [decision log](decisions.md#2026-11-02).

### B3 — Shown (S–M)

- The 3D view, the joint close-up and the face view draw each profile
  of a built-up member, with the gap.
- The joint details see its parts: a bolt goes through every ply (one
  more shear plane per ply), a filler plate fills the gap at the ends.

### B4 — The rules for built-up members (M): calculated as built up

- Design data per member: the spacing of the stitch bolts (or their
  number along the member) and whether the parts are tied at the ends.
- ASCE 10-97's rules for built-up members (each part's slenderness
  between the stitches against the whole member's; the connectors'
  spacing): the clauses' text from the owner, none typed from memory,
  as for the rest of the ASCE 10-97 check.
- The finite-element model keeps one bar with the built-up section's
  properties (the parts acting together through the stitches): said as
  an assumption in the theory pages.

### B5 — Joints of built-up members (S–M, with the joint details' stage 4)

The connection of a built-up member: the bolts through both plies, the
filler plate, a star's two angles each bolted to its own leg face; the
bolt capacities per ply.

## Order

B1 first (the sections calculated from their parts, checked against the
tower program's values), then B3 (shown) and B2 (imported), then B4 and
B5 with the joint details' stage 4.

## Questions

1. The gap when the data has none: the thickness of the joint's gusset
   plate, a typed default per section, or required?
2. A star's angles: equal legs only, or unequal too?
3. B4: should a built-up member without stitch data be checked as one
   section (as today) or with the least favourable assumption (the parts
   buckling on their own over the whole length), said per member?

> **Owner:**
