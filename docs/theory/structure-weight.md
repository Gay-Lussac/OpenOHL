# Weight and quantities of a lattice tower

*Built 2026-11-19 (joint details stage 4.6), in active development, not
approved yet.* How OHL-Structure counts and weighs what a lattice tower is
built from — its members, gusset plates, splice covers, bolts and welds —
makes its bill of materials, and when a run takes the counted mass as the
tower's self-weight instead of the members' steel × the dead-load factor.
The code: `ohl_bom::structure` (the rules, checked by hand on invented
parts), `ohl_structure_link::structure_quantities` and
`counted_self_weight` (the counting and the switch); how to use it:
[OHL-Structure › Weight & quantities](../user-guide/structure-app.md#weight--quantities).

## Why

Until every joint is defined, a run takes the members' steel (each its
cross-section × its length between its joints) × the dead-load factor
`[analysis] dead_load_factor` (bolts, plates, galvanising: typically 1.1 to
1.4 on lattice towers, more on light bracing). The factor is an estimate.
Where the joint details give every plate and bolt, they can be weighed
instead (owner, 2026-11-02: "the self-weight factor stays until every
joint is defined").

## Two counts

Every part has two masses:

- the **heavy count**: each doubt taken on the heavy side — the weight a
  run takes where it loads the tower (members in compression, the
  foundation's compression);
- the **light count**: each doubt taken on the light side — the weight a
  run takes where it holds the tower (uplift of a foundation,
  overturning).

## What is counted

Only from the connection records: a member end without a
`[[connection]]` is a **joint not defined**, and that joint's parts are not
counted. A joint the automatic joint layout left for a hand detail, a
bolted end without bolts (neither its own nor its member's design data)
and a plate or cover without an outline make the joint not defined too.
What the automatic layout laid out (its provisional bolts, the plates it
made) is counted and marked *automatic*.

| Part | Heavy count | Light count | Source |
|---|---|---|---|
| Member | its cross-section's area × its material's density × its length between its joints (the model's self-weight), plus its steel beyond the work points where the joint details draw it so | its area × density × its steel length (short of the work points by the joint details' cut, given or worked out), less the holes of the ends counted (hole × the leg bolted's thickness) | the cross-section library's material |
| Gusset plate | its outline's area (given, or worked out round its bolts) × its thickness × the density of the heaviest member bolted on it; **holes not deducted** | the same less its holes (each bolt of the ends bolted onto it, its hole, else its diameter) | the members' material; nothing bolted on it: SIA 261:2014, Annexe A, Table 28, steel 78.5 kN/m³ (÷ 9.80665 m/s² = 8 004.8 kg/m³) |
| Splice cover | as a plate, with the members of the splice | as a plate, the holes of the ends on its leg | as a plate |
| Bolt (library mass) | the bolt library's `mass_kg` ("with its nut and washer", as sold), plus the shank beyond its library grip where it clamps more | the same | the bolt library (none of a standard's values built in) |
| Bolt (no library mass) | weighed from its sizes: the shank (nominal diameter) over the grip, the nut and what stands beyond it; head and nut as hexagonal prisms; the nut's hole not deducted, the threads not reduced | the same | the library's head and nut sizes, else the drawing proportions (1.5 d across the flats, head 0.65 d, nut 0.8 d, 0.3 d beyond the nut) — **an assumption**, no standard's dimensions |
| Fillet weld | section × length × density (below) | section × length × density (below) | SIA 263:2013 6.3.2.5, 6.3.2.6, 6.3.6.3; the filler as steel by SIA 261 Table 28 |
| Full penetration weld | the thinner plate's thickness squared × the length (**an assumption**: a prepared groove filled) | none (the plates' outlines meet at the seam already) | |

### A fillet weld

The weld's size is its throat \\( a \\), the height of the triangle
inscribed in its section (SIA 263:2013 6.3.2.6, Figure 34). Taken
isosceles in the corner of apex angle \\( \varphi \\) between the plates,
its section is

\\[ A_w = a^2 \tan\frac{\varphi}{2} \qquad (a^2 \text{ at } 90°). \\]

The data do not say on which side of the seam the fillet lies: the heavy
count takes the more open corner (\\( \varphi \\) the larger of the angle
between the plates and its supplement, at most 135°), the light count the
closer one (at least 45°). The two limits are **an assumption**: plates
meeting flatter take a butt weld, not a fillet.

- **The length**: given, it is the whole length welded (SIA 263 6.3.2.5,
  "la longueur totale du cordon réalisé"; both sides together when welded
  on both), the same in both counts. Not given: the seam's length where the
  two plates meet, **both sides** in the heavy count, one side in the light
  count.
- **No size given**: the heavy count takes the largest throat SIA 263
  6.3.6.3 allows, 0.7 × the thinner plate's thickness; the light count
  leaves the weld out.

## Assumptions and what is not counted

**On the safe side for the heavy count** (the weight loading the tower):
holes not deducted in plates, covers and members; a member's steel beyond
the work points added; the drawing's head and nut sizes and the nut's hole
not deducted for a bolt without a library mass; a weld without a size at
the largest throat allowed, both sides; the open corner of a fillet; a
plate with nothing on it in SIA 261's steel (heavier than the 7 850 kg/m³
libraries often give).

**NOT ON THE SAFE SIDE for the heavy count** (said in the bill's notes and
in the run's note):

- galvanising, paint, fillers and packs (no record in the data), step
  bolts, ladders, anti-climbing guards and other fittings not modelled as
  parts on the structure (parts and assemblies on the structure keep
  their own loads);
- washers of a bolt weighed from its sizes (a library mass includes them);
- a plate or cover outline worked out round its bolts: the least plate
  keeping the edge distances — a drawn plate is often larger;
- the welds of a welded member end and what holds a pinned or fixed end
  (no size in the data);
- a library bolt's mass where the library gives no grip ("as sold").

**NOT ON THE SAFE SIDE for the light count** (the weight holding the
tower): the members' steel taken to the joint details' cut, which is
worked out where not given; a library bolt's mass as sold, the same in
both counts.

## The self-weight from the counted mass

The counted mass enters a run's self-weight only when

1. every joint is defined: every member end (not a modelling member, not a
   cable) has its `[[connection]]` — the core's level 4 rule,
   `StructureType::ends_without_joint_details` — none is left for a hand
   detail, every bolted end has its bolts and every plate its outline;
2. the tower's own members are solved as written: a separate model file
   is not counted, and an extension with its own members keeps the factor
   (its joints are not counted).

Else the members' steel × the dead-load factor stays, and the run says
why, per structure: "factor kept: N joint(s) not defined", "factor kept:
extension …".

**The conservative envelope (the default, until the owner validates the
counting).** The counting is not validated against real tower weights and
leaves out galvanising, fillers and packs and fittings. So the counted
mass may only **raise** a load, never lower one (coordinator, 2026-11-19,
pending the owner's answer):

- where the weight loads the tower (a case's self-weight factor of 1 or
  more): \( \max(M_{heavy},\ M_{members} \cdot f) \), with \( f \) the
  dead-load factor (the members' own factors where given: their total);
- where the weight holds the tower (a factor below 1): \( \min(M_{light},\
  M_{members} \cdot f) \) — less holding weight than, or the same as,
  before.

Where the counted mass governs where the weight loads the tower, every
member's self-weight is multiplied by \( k = M_{heavy} / M_{members} \)
instead of the dead-load factor (the members' own factors dropped), spread
over the members as the factor spread it; a case with a factor below 1
then takes its factor × the lesser mass over the mass the model carries.
The run's note says which value governed, for example: "counted mass
1.155 × the members' steel, below the dead-load factor 1.3 (1.300 ×):
factor kept (counting not validated); the cases whose self-weight factor
is below 1 … take the light count …".

**The other choices** (`self_weight_from`, structure file version 19;
Design › Weight & quantities):

- `"factor"` — the members' steel × the dead-load factor, as before; a run
  says **NOT ON THE SAFE SIDE** where the counted heavy mass is above it;
- `"counted"` — the counted mass alone (opt-in, off by default): the heavy
  count where the weight loads the tower, the light count where it holds
  it; marked **NOT ON THE SAFE SIDE** until the counting is validated (it
  may lower the dead load below the members × the factor, and galvanising,
  fillers and fittings are not in it).

### Where the weight holds the tower

SIA 260:2013 4.4.3, Table 1, gives the permanent actions two load factors:
\\( \gamma_{G,sup} \\) where their effect is unfavourable (1.10 for type 1,
the overall stability — "déversement, soulèvement"; 1.35 for type 2) and
\\( \gamma_{G,inf} \\) where it is favourable (0.90 and 0.80); "G est
multiplié par γG,sup ou par γG,inf, selon que l'effet d'ensemble de
l'action est défavorable ou favorable" (footnote 1). LeV (SR 734.31,
2025-08-01), Anhang 14 Ziff. 2.2, asks for the structure's own weight in
every load assumption; Anhang 13 Ziff. 1.8 gives steel towers one safety
factor of 1.4 (1.1 on the resistance, 1.3 on the loads) without a
favourable factor. No code plugin sets the structure's self-weight
factor: a load case recipe does (its `self_weight`, typed after the
code).

The counted mass is the characteristic weight \( G \); a case's factor
applies to it as it did to the members × the dead-load factor. A case
whose self-weight factor is below 1 (a favourable \( \gamma_{G,inf} \))
is taken as one where the weight holds the tower; a case with a factor of
1 or more as one where it loads it — **NOT ON THE SAFE SIDE** for a case
typed 1.0 where the weight is favourable (LeV's single factor, a case
typed 1.0 for uplift): with the envelope it keeps at least the members ×
the factor, as before this work.

**Against today's results.** With the envelope no load falls: where the
weight loads the tower the dead load is today's or more; where it holds
the tower the holding weight is today's or less (the light count is
usually below the members × a typed factor of 1.15 to 2.4: those cases'
usages rise — the typed factor was an estimate on the heavy side, so for
them it was **not on the safe side**). With `"counted"` the dead load
falls where the counted mass is lighter than the members × the factor
(the measured towers below: about 1.1 to 1.2 against the typed 1.15 to
2.4), said not on the safe side.

**The line's checks** (OHL-Design, `ohl structures fe`) build the
structure's model in the core, which has no joint details: they keep the
dead-load factor. Only OHL-Structure's runs and reports take the counted
mass.

## Measured (2026-11-19)

- **Shipped examples**: no example tower has joint details; their bills
  are their members alone, and the factor stays everywhere.
- **73 real lattice towers** of one line (local, numbers without names),
  after Orient angles and Lay out joints: **none has every joint
  defined** (the layout leaves the legs' ends and the joints needing a
  hand detail without records: 59 554 of 64 491 joints not defined), so
  no run switches. Counted so far (the defined parts only): 42 881 plates
  (101 t, 22 899 outlines worked out), 171 956 bolts (37.7 t, all
  weighed from their sizes: the tower bolt library gives no masses), no
  welds or covers. The heavy count so far is 1.000 to 1.116 × the
  members' steel (median 1.048), the light count 1.000 to 1.052 (median
  1.029: the members to where the joint details cut them). The plates and
  bolts scaled by the share of member ends with a record (a rough guide,
  not a count; one tower has none): up to 1.20 (median 1.14), against the
  files' dead-load factors 1.15 to 2.4 (median 1.3); 13 of 73 towers above
  their factor.
- Counting a tower of 1 000 joints takes 0.33 s (median; at most 0.47 s).

## Validation

Hand calculations of invented parts (`ohl_bom::structure` tests: a
plate, holes, a fillet at 90° and 120°, a bolt from its sizes, the bill's
sums) and of an invented joint through the whole counting
(`ohl-structure-tests/tests/structure_weight.rs`: a gusset plate, a plate
welded to it, a weld, a splice cover, six library bolts; a joint not
defined; the run's dead load from the heavy and light counts, and the
factor kept).
