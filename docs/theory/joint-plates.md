# Gusset plates and their welds

Implemented in `crates/ohl-structures-fe/src/plates.rs` (`bolted`, `weld`,
`contact_width`, `PlateCheck`, `Checked::governing`) with the geometry and
forces of `apps/structure/ohl-structure-link/src/plate_checks.rs`
(`plate_plans`, `plate_usage`, the weld's force, the distances to the
plate's edge), run with every case of OHL-Structure's analysis (Results ›
Run) and shown in Joint details and the structure report. Built, in
active development (not approved yet); pre-alpha: results must be
verified by a qualified engineer.

**Sources.** ASCE 10-97, *Design of Latticed Steel Transmission
Structures* (American Society of Civil Engineers), and SIA 263:2013,
*Construction en acier* (Swiss Society of Engineers and Architects).
Clause, table, figure and equation numbers below are the standards'. The
plates are checked by the structure's capacity method (`[analysis]
capacity`); the **simple check** checks neither plates nor welds (said in
the results).

Every assumption below is marked **on the safe side** (conservative) or
**not on the safe side**.

## What is checked, per case

For each gusset plate (`[[plate]]`) and each case solved:

- **per member bolting onto the plate** (`onto` the plate): its bolts'
  bearing on the plate, the plate's block shear (tearing) round the
  member's bolt group, the plate's section across the bolt group, and the
  plate's end and edge distances from those bolts;
- **per weld between two plates** (a welded plate,
  [`[[plate.weld]]`](../development/data-model.md)): the force it carries
  against its resistance.

The force is the member's **axial force** at the joint from the analysis
(tension positive). A frame member's end shear and moments are not taken
into the plate checks (**not on the safe side** for frame members; said
per member). The case's strength factor \\( \phi \\) multiplies every
resistance, as for the members.

A part's usage is its largest share over its checks. A distance is kept
or not: it counts only when it is not kept (its share above 1), never as
a share of a resistance.

## The bolt group in the plate

![Left: a member in tension bolted onto a gusset plate by two gauge lines of three bolts, with the pitch p, the span between the outer lines, the end distance e1 to the plate's edge in the force's direction, the edge distance e2, and the block shear path: two shear planes along the outer gauge lines and the tension plane between them. Right: a fillet weld between plates at θ with its throat a and contact width s](../images/theory/joint-plate.svg)

From the bolts where Joint details places them (the connection's layout
or bolt points, in the plate's plane):

- **gauge lines**: the bolts' places across the member (within 2 mm);
  bolts per line, the **pitch** \\( p \\) (the least between two bolts on a
  line) and the **span** between the outer lines;
- **end distance** \\( e_1 \\): from the bolts to the plate's outline
  along the member, in the force's direction — away from the joint for a
  member in tension, towards it in compression; the least over the bolts
  (**on the safe side**: the bolt nearest the edge);
- **edge distance** \\( e_2 \\): from the bolts to the outline across the
  member, the nearer side, the least over the bolts;
- **the width the section crosses** \\( w \\): neither standard gives a
  gusset plate's effective width. Taken as the bolt group's span plus the
  code's minimum edge distance on each side (ASCE 10-97 4.5.3, SIA 263
  Table 17 \\( e_2 \\)), or the plate's own width through the bolts when
  narrower (**on the safe side**: the plate is at least that wide where it
  keeps the code's edges, and no spreading of the force into the plate —
  a Whitmore section — is counted; neither standard gives one);
- the hole \\( d_0 \\): the connection's, else the standard hole of each
  code; the plate's steel: its material in the section library, else the
  weakest steel of the members bolted onto it (**assumed**, said; not
  necessarily on the safe side if the plate is of a weaker steel than
  every member onto it).

The bolts are taken to share the force equally (ASCE 10-97 4.1).

**A member bolted onto two plates** (both legs bolted, the other leg onto
a plate of its own — `other_onto`, two plates at 90°, apart or welded;
2026-11-08): each plate is checked for the member's force times its share
of the end's bolts, \( n_\text{plate} / n_\text{end} \) — the same equal
sharing among the end's bolts (**an assumption**, said in the check: it
holds when the legs are alike in stiffness and the bolts slip alike; it is
**not on the safe side** for a plate whose leg draws more of the force, and
neither standard gives the split between an angle's two legs).

## ASCE 10-97

| Check | Resistance | Source |
|---|---|---|
| bearing | \\( n \cdot 1.5\,F_u\,d\,t \\), \\( F_u \\) the lower of the plate's and the bolt's | 4.4 |
| block shear | \\( P = 0.60\,A_v F_u + A_t F_y \\) | 3.10.2, Eq. 3.10-1 |
| net section (tension) | \\( F_y\,t\,(w - n_\text{lines}\,d_h) \\) | 3.10.1 |
| end distance | \\( e_1 \ge \max(1.2P/(F_u t),\ 1.3d,\ t + d/2) \\), \\( P \\) the force per bolt | 4.5.1, Eqs. 4.5-1 to 4.5-3 |
| edge distance | \\( e_2 \ge 0.85\,e_\text{min} + 1/16 \\) in. | 4.5.3, Eq. 4.5-7 |

- \\( d_h \\): the hole + 1/16 in. (1.6 mm), holes taken as punched
  (3.10.1, 3.10.2; **on the safe side** for drilled holes).
- **Block shear**: the standard states Eq. 3.10-1 for angles bolted by
  one leg. The plate's block is taken with the same formula: two shear
  planes along the outer gauge lines from the plate's edge to the
  innermost bolt, \\( A_v = 2t\,(e_1 + (n-1)p - (n - \tfrac12)d_h) \\), and
  the tension plane between the outer lines,
  \\( A_t = t\,(\text{span} - (n_\text{lines} - 1)d_h) \\). This
  **applies a rule the standard gives for angles to a plate** — said in
  every result. Other tearing paths (towards a side edge of the plate)
  are not checked (**not on the safe side** for a bolt group near a side
  edge).
- **Net section**: 3.10.1 is the rule for tension members; the plate's
  section across the bolt group is checked by it in tension only.
- **Edge**: the plate's edges taken as sheared or flame-cut (Eq. 4.5-7,
  the larger edge distance; **on the safe side**).
- **Welds**: ASCE 10-97 gives **no weld resistance** — 5.2.5 leaves
  welding procedures to ANSI/AWS D1.1. Welds are **not checked** under
  ASCE 10-97, said in the results (no rule is invented).
- **Compression**: the standard gives no rule for a gusset plate in
  compression: "plate buckling not checked", said.

## SIA 263:2013

Design values with \\( \gamma_{M1} = 1.05 \\), \\( \gamma_{M2} = 1.25 \\)
(4.1.3).

| Check | Resistance | Source |
|---|---|---|
| bearing | per bolt \\( F_{b,Rd} = 0.85\,\dfrac{e_1}{d_0}\,\dfrac{f_u}{\gamma_{M2}}\,d\,t \le 2.4\,\dfrac{f_u}{\gamma_{M2}}\,d\,t \\) | Table 16, (76) |
| net shear | \\( V_{eff,Rd} = \dfrac{0.9 f_u}{\gamma_{M2}\sqrt3} A_{v,net} \\) | 6.2.4.3 a), (82), Figure 28 |
| block shear | \\( V_{eff,Rd} = \dfrac{0.9 f_u A_t + \dfrac{f_y}{\sqrt3} A_{v,brut}}{\gamma_{M2}} \\) | 6.2.4.3 b), (83), Figure 29 |
| gross section | \\( f_y A / \gamma_{M1} \\), \\( A = w\,t \\) | 5.1.2.1, (38) |
| net section (tension) | \\( 0.9 f_u A_{net} / \gamma_{M2} \\), \\( A_{net} = t\,(w - n_\text{lines}\,d_0) \\) | 5.1.2.1, (39) |
| end and edge distances | \\( e_1 \\), \\( e_2 \\) of Table 17 by the bolt | Table 17 |

- **Bearing**: \\( e_1 \\) the plate's end distance in the force's
  direction; with several bolts a line and a pitch
  \\( p_1 < e_1 + d_0/2 \\), \\( e_1 \\) replaced by \\( p_1 - d_0/2 \\)
  (Table 16) for every bolt (**on the safe side**). (76) asks
  \\( e_2 \ge d_0 \\) and \\( p_2 \ge 2 d_0 \\); when the plate does not
  meet them it is said.
- **Net and block shear** (6.2.4.3): the block round the bolt group torn
  out towards the plate's edge in the force's direction:
  \\( A_{v,net} = 2t\,(e_1 + (n-1)p - (n - \tfrac12)d_0) \\),
  \\( A_{v,brut} = 2t\,(e_1 + (n-1)p) \\),
  \\( A_t = t\,(\text{span} - (n_\text{lines} - 1)d_0) \\); both criteria,
  the lower governing. Other paths: as for ASCE 10-97, not checked.
- **Gross section** (38) in tension and compression; the net section (39)
  in tension only (6.2.4.1: in compression the holes are not deducted).
- **Compression**: SIA 263 gives no rule for a gusset plate's buckling
  (its 4.5.3 effective widths are for the elements of members): "plate
  buckling not checked", said.

### Fillet welds (6.3.2)

The weld's **size is its throat \\( a \\)**: the height of the triangle
inscribed in the weld's section (6.3.2.6, Figure 34; "la dimension de la
gorge \\( a \\) est en général désignée comme l'épaisseur du cordon").
The resultant \\( F_{Ed} \\) per unit length is checked on two sections
(6.3.2.2), regardless of its direction (6.3.2.1):

\\[ F_{Ed} \le a\,\Delta L\,\frac{f_w}{\gamma_{M2}},\quad f_w = 0.6\,f_{uE}
\qquad (87) \\]

\\[ F_{Ed} \le s_{min}\,\Delta L\,\frac{f_s}{\gamma_{M2}},\quad f_s = 0.8\,f_y
\qquad (88) \\]

- \\( f_{uE} = 510 \\) N/mm², the value 6.3.2.2 gives "en général". For
  high-strength steels 6.3.2.3 asks a filler matching the base metal;
  510 N/mm² is kept (**on the safe side** when the filler is stronger).
- \\( f_y \\): the lower of the two plates' (**on the safe side**).
- \\( s_{min} \\): the contact width of the inscribed triangle, taken
  isosceles (equal legs): \\( s = a / \cos(\theta/2) \\) with \\( \theta \\)
  the smaller of the angle between the plates and its supplement (the
  fillet may lie on either side; at 90°: \\( s = a\sqrt2 \\)). The smaller
  value is **on the safe side**.
- **The length** \\( L \\): the length given (6.3.2.5: the whole length
  welded; both sides together when welded on both); none given: the
  seam's length where the two plates meet, worked out from their
  outlines, **one side only** (**on the safe side** for a weld on both
  sides).
- **The force spread evenly along the weld**, \\( F_{Ed} = F / L \\), as
  6.3.1.7 allows ("une distribution simplifiée des contraintes"). The
  moment of the force about the seam is **not taken** (**not on the safe
  side**: a resultant acting off the seam's middle loads one end more).
- Said when broken: a length below 40 mm (6.3.6.2), \\( a \\) above 0.7
  times the thinner plate (6.3.6.3), a plate thinner than 4 mm (6.3.1.3:
  the chapter does not apply), \\( a \\) below the least recommended for
  one pass by the thicker plate (6.3.6.4). 6.3.5.1's reduction for long
  lap joints is not applied (a seam between two plates is not a lap
  joint).
- A **full penetration** weld is as strong as the weaker plate (6.3.3.4)
  and the standard asks a check in general only for fillet welds
  (6.3.1.8): not checked, said. A weld **without a size** is not checked,
  said.

### The force a weld carries

**On the safe side**: the resultant of the member forces on the plates it
holds. The welded plate is split at the weld; on each side the members
bolted onto its plates give \\( \mathbf R = \sum N_i\,\mathbf u_i \\)
(\\( \mathbf u_i \\) the member's direction from the joint, \\( N_i \\)
tension positive); the weld carries the larger \\( |\mathbf R| \\) of the
two sides. A plate on that side also held otherwise (bolted onto the
joint's leg) passes part of it there, so the weld carries no more. Welds
closing a ring (no split): the sum of every member's \\( |N_i| \\) on the
welded plate (more on the safe side). The joint's legs' own forces are not
counted (they do not bolt onto the plates).

## Results

Each part per case: its usage, what governs with its clause, every check
and what was assumed. Joint details lists the joint's parts (the case
shown in Results and the largest over the cases), the 3D view marks each
joint with plates by its worst usage (the members' colours: orange from
90 %, red above 100 %), the structure report lists every part's largest
usage over the cases and the governing case.

## Validation

Unit tests in `crates/ohl-structures-fe/src/plates.rs`, by hand on
invented data:

- `asce10_plate_by_hand`: four M16 bolts (hole 18 mm, punched 19.6 mm)
  in two lines 60 mm apart, two to a line 50 mm apart, 40 mm to the
  plate's edge, a 10 mm S355 plate: bearing 489.6 kN, block shear
  514.3 kN, net section 286.84 kN (ASCE 10-97).
- `sia263_plate_by_hand`: the same plate by SIA 263: bearing 493.2 kN,
  block shear 267.1 kN (82) and 449.3 kN (83), gross and net section
  405.7 kN and 308.4 kN.
- `a_fillet_weld_by_sia263`: a 6 mm fillet weld 220 mm long between
  plates at 90°: 323.1 kN (87) and 424.1 kN (88); by ASCE 10-97 not
  checked (it has no weld rule), and said.
- `a_weld_carries_the_larger_resultant`, `rays_reach_the_outline`: the
  force a weld carries and the plate's outline.
