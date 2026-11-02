# Built-up members (plan)

*Plan, 2026-11-02 — not built, not approved. Asked by the owner: "plan
the double star and 4 corners to be shown or at least calculable".
Open point O20 in the [roadmap](../roadmap.md); comment there or under
each stage.*

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

### B2 — Importing them (S)

The local importer of the tower program's files maps its built-up
angles to the new shapes (the arrangement and the gap from its data,
when it gives them), keeping its values in the description to compare.

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
