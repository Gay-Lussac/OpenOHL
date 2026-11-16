# Wind on the line: its direction and its load

The wind of a weather case or load case on everything the line computes:
where it comes from, what each wire, string, object and structure takes of
it, and what is exact, conservative or not. The pressure itself and the
iced diameter are in [Sag-tension › Weather loads](sagtension.md#weather-loads);
the wind on lattice bodies (faces, solidity, shielding, the force on an
inclined member) in [Wind on structures](structure-wind.md).

Implemented in `crates/ohl-weather/src/direction.rs` (`WindDirection`,
`ObliqueLaw`, `oblique_wire_force`), `crates/ohl-line/src/wind.rs`
(`WireWindOptions`, `PlaceWind`, the line's direction at each span and
point), the line solve (`crates/ohl-line/src/solve.rs`:
`oblique_span_load`, the strings, objects, spacers, the ruling span),
jumpers and posts (`jumper.rs`, `post.rs`) and the load cases
(`crates/ohl-loads`: `LoadCase::wind_direction`, `wind_directions`,
`expand_directions`, `structure_wind`, `in_structure_axes`). Owner,
2026-11-15: *"it would be nice to be able to have in the weather cases the
possibility to have any angle with 0° being along the alignment and 90°
right of the alignment"*. Built, in active development (not approved yet).

## The rule for existing cases: identical results

A case **without a direction**, or with one straight across the line —
`right` or 90°, `left` or 270°, and every older file's `wind_from` — keeps
the **across rule** described below, and gives the same results as before
the direction existed, **bit for bit** (owner, 2026-11-15: *"the current
cases need to be identical"*). This holds for every weather case file of
version 1, every load case file up to version 6 (their structure-body
angles are read in their own convention, below), and OHL-Structure's
files (unchanged). Checked by a regression test that solves the example
lines in all their weather and load cases and compares every number
written with the program before the change
(`crates/ohl-cli/tests/wind_direction_identical.rs`).

## Where the wind comes from

![Plan view of an angle structure with a 20° line angle: the line's direction ahead along the bisector, the angles counted clockwise from it — 0° from ahead, 90° from the right, 180° from behind, 270° from the left — and a wind from 45° blowing back and to the left; beside it the rules in words](../images/theory/wind-angles.svg)

*The direction θ: where the wind comes from, clockwise from the line's
direction ahead (at a structure, the bisector of its line angle).*

One convention in the whole program. The angle \\( \theta \\) names where
the wind comes **from** (as a wind rose does), relative to the line's
direction at the place, counted **clockwise seen from above**:

| θ | from | blows |
|---|---|---|
| 0° (`ahead`) | ahead | back along the line |
| 90° (`right`) | the right | the wires to the left — the old "from the right" |
| 180° (`back`) | behind | ahead along the line ("forward") |
| 270° (`left`) | the left | the wires to the right — the old "from the left" |

With \\( \hat a \\) the line's direction ahead (in plan) and \\( \hat r =
(a_y, -a_x) \\) the direction to its right, the wind comes from
\\( \hat f = \hat a\cos\theta + \hat r\sin\theta \\) and blows towards

\\[ \hat w = -\hat f = -\hat a\cos\theta - \hat r\sin\theta. \\]

**The line's direction at a place** (`Line::direction_at`): the nearest
segment of the line's routes; at an angle point the **bisector** of the
two segments. A **span** takes the direction at its middle (its route
segment), a **structure** (its strings, posts, jumper supports, its body)
the direction at its position (the bisector at an angle structure). The
wind therefore turns with the line from one route segment to the next: it
is a design wind relative to the line, as codes give it, not one compass
direction for the whole line. A line without routes takes each section's
direction from its first to its last point.

A section entered against the line's direction takes the same wind as the
same section entered along it: the wind is the line's, not the section's.

**Old angles.** Load case files up to version 6 and OHL-Structure count a
structure-body angle the other way: from; 0° from the right, counted
towards ahead (90° from ahead). In the program's convention

\\[ \theta = (90° - \theta_{old}) \bmod 360°. \\]

Such an angle is kept **as read** (`WindDirection::OldAngle`), so that its
forces are computed with the old formula \\( (-\cos\theta_{old},
-\sin\theta_{old}) \\) bit for bit — \\( 90° - \theta_{old} \\) in floating
point does not always give back the same last digit. A file is written
back at its version with the angle as read; raised to version 7 (another
case uses something new) the angle is converted. The names (`right`,
`ahead`, …) mean the same in both conventions.

## The across rule (no direction, 90°, 270°)

Every span takes the case's **full** pressure square to itself in plan,
pushed to the side away from where the wind comes from, whatever the span's
direction; strings, objects, jumpers and posts the same across the line.
**Conservative** for the transverse loads and the side clearances: a real
wind from one direction gives a span at an angle to it only the part
normal to it, never more; the across rule is the envelope of the one-wind
loads below for every horizontal wind across the line
([Conventions](conventions.md)). For a span parallel to the line it is
exactly the one wind from 90° (or 270°).

Codes that prescribe the wind on the conductors at an angle point
"in the direction of the bisector" (LeV Anhang 14 Ziff. 3.1, *"Für
Tragwerke in Winkelpunkten der Leitung wirken die Windkräfte auf die Leiter
und das Tragwerk in Richtung der Winkelhalbierenden"*) are the across
case: every span the full pressure, the structure's resultant along the
bisector.

## One wind at an angle: the load per span

![A span's chord with its tangent t; the wind blowing towards w at the angle θ to the wire; its part normal to the wire, of length sin θ, and its part along it, cos θ; the normal force by the projected area and by the cross-flow principle, the friction along the wire](../images/theory/wind-normal.svg)

*A span takes the wind's part normal to it; the friction option adds a
force along it.*

For a span with unit tangent \\( \hat t \\) (its **3D chord**: inclined
spans included) and the wind towards \\( \hat w \\), the wind's part
normal to the wire is

\\[ \mathbf w_n = \hat w - (\hat w\cdot\hat t)\,\hat t, \qquad
|\mathbf w_n| = \sin\theta, \\]

\\( \theta \\) the angle between the wind and the wire. With \\( qD \\) the
pressure on the iced diameter (N/m), per metre of the wire as it hangs
(stretched, [Finite-element wire systems](wirefe.md)):

- **Projected area (the default):** \\( \mathbf q = qD\,\mathbf w_n \\),
  \\( |\mathbf q| = qD\sin\theta \\). The force normal to the wire on the
  area it shows to the wind. SIA 261:2014 Tableau 74 (*Coefficients pour
  treillis spatiaux*, read on page 112) gives for **round bars** the force
  normal to an inclined bar, \\( Q_\beta = c_\beta\,q_p\,d\,l \\), with
  \\( c_\beta \\) = 1.2, 1.16, 1.04, 0.85, 0.6 at β = 0°, 15°, 30°, 45°,
  60° (β between the wind and the bar's normal, \\( \beta = 90° - \theta \\);
  the Table's other range of \\( d\sqrt{q_{p0}} \\): 0.6, 0.58, 0.53,
  0.42, 0.28) — \\( 1.2\cos\beta \\) to within 1 %, that is \\( \sin\theta \\).
  LeV Anhang 15 Ziff. 1.4 says it in words: *"Für die Berechnung der
  Windkräfte ist die senkrecht zur Windrichtung liegende Ansichtsfläche
  massgebend"* (the area seen square to the wind, \\( DL\sin\theta \\)).
  **Conservative** compared with the cross-flow principle (larger for
  every angle).
- **Cross-flow principle (the option):** \\( \mathbf q = qD\,|\mathbf
  w_n|\,\mathbf w_n \\), \\( |\mathbf q| = qD\sin^2\theta \\). The flow round a
  yawed cylinder is set by the velocity's normal component, so the normal
  force is that of a cross flow of speed \\( V\sin\theta \\): the
  *independence principle*, observed by E. H. Relf & C. H. Powell (1917),
  *Tests on smooth and stranded wires inclined to the wind direction and
  a comparison of the results on stranded wires in air and water*,
  Aeronautical Research Committee, Reports and Memoranda 307, for
  10° < θ < 90° — **as reported in** L. Divaret, O. Cadot, P. Moussou, O.
  Doaré (2014), *Normal forces exerted upon a long cylinder oscillating
  in an axial flow*, J. Fluid Mech. 752, 649–669,
  doi:10.1017/jfm.2014.342, §1 (R&M 307 itself not read here). Divaret et
  al. measure that the law holds above about 5° and that below it the
  force grows linearly with the angle, more than \\( \sin^2 \\) (their
  §2, a rigid cylinder at a Reynolds number of 24 000). **Simplification** compared
  with the code tables (lighter than SIA 261 Tableau 74 for round bars);
  **not on the safe side** below about 5° to 10°.

Both laws give the full \\( qD \\) for a wind square to the wire and
nothing for a wind along it. *Line › Criteria › Calculation options ›
Wind at an angle on the wires* chooses the law (the line file's
`solver.wind.law`, `projected` or `cross_flow`; OHL XML `<solver
wind_law>`); it changes nothing in a case without a direction.

Hand figures (the plan's): a level 300 m span, \\( D \\) = 30 mm,
\\( q \\) = 500 Pa, so \\( qD \\) = 15 N/m:

| wind to wire θ | 90° | 60° | 45° | 30° | 10° | 0° |
|---|---|---|---|---|---|---|
| projected, N/m | 15.00 | 12.99 | 10.61 | 7.50 | 2.60 | 0 |
| cross-flow, N/m | 15.00 | 11.25 | 7.50 | 3.75 | 0.45 | 0 |
| on 300 m (projected / cross-flow), N | 4500 / 4500 | 3897 / 3375 | 3182 / 2250 | 2250 / 1125 | 781 / 136 | 0 / 0 |

### The vertical part on an inclined span

A horizontal wind on an inclined wire has a normal part with a vertical
component: a wind blowing **up** the slope presses the span **down**, one
blowing **down** the slope lifts it. Included (owner, 2026-11-15): leaving
it out is not on the safe side for uplift and weight spans. A span rising
30°, the wind along it: \\( \mathbf w_n \\) of length \\( \sin 30° \\);
cross-flow \\( 15 \times 0.5^2 \\) = 3.75 N/m normal to the wire, of which
3.25 N/m vertical. For a wind across the line it is 0: the across rule's
results are unchanged.

### Friction along the wire (option, off by default)

Owner, 2026-11-15: *"please add an option for the friction on the
wire"*. Neither law above has a force **along** the wire. With the option
*Friction along the wire* (coefficient \\( c_t \\); the line file's
`solver.wind.friction`, OHL XML `<solver wire_friction>`), each span takes
besides its normal force

\\[ \mathbf q_t = c_t\,qD\,(\hat w\cdot\hat t)\,|\hat w\cdot\hat t|\,\hat t
= c_t\,qD\cos^2\theta \text{ along the wire, with the wind}. \\]

What is assumed: the force along the wire is the dynamic pressure of the
wind's speed **along** the wire, \\( q\cos^2\theta \\), on the (iced)
diameter, times the user's coefficient — the form of a skin-friction drag
on the axial flow. Divaret et al. (2014, §1) report that G. Taylor
(1952, *Analysis of the swimming of long and narrow animals*, Proc. R.
Soc. Lond. 214, 158–183; not read here) modelled the friction on a yawed
cylinder with a friction
coefficient, that such coefficients have been given by several authors
and that practice often takes different ones for the normal and the
axial direction; they give no value for a stranded wire in air. **The
program gives no value**: the coefficient is to be given by the user,
from a source for the wire and the wind concerned (its referred area here
is the diameter, per metre of wire). Off (the default), the results are
those without it, bit for bit.

**Not on the safe side** without the option: the longitudinal pull a wind
along the line puts on dead-ends and angle structures through the wires is
left out.

### Strings, objects, spacers, jumpers, posts

- **Strings** (suspension, strain, V legs): the full pressure × the
  string's `wind_area` along \\( \hat w \\) at its structure (the
  bisector's direction there). Exact for a plumb string (square to every
  horizontal wind); **Conservative** for an inclined or strain string (no
  reduction for its angle to the wind).
- **Objects on the wires** (marker balls, spacers' bodies, given areas):
  their area as given, along \\( \hat w \\) of their span.
  **Simplification:** a cylinder's or box's area is the one given (across
  its span); a body showing more area to a wind along the span takes too
  little (not on the safe side), one showing less too much.
- **Spacers between sections:** their wind area along \\( \hat w \\) at
  the span's middle, full pressure.
- **Jumpers:** each piece takes the one wind by the law, its own 3D
  direction for \\( \hat t \\), with the friction option. A jumper across
  the line under a wind along the line takes its full wind; one along the
  line none. (The across rule keeps the envelope: every piece the full
  pressure square to it.)
- **Posts and hinged posts' members, jumper support strings:** the full
  pressure on their one area along \\( \hat w \\): **Conservative** (no
  reduction for a post's angle to the wind).
- **Local winds per span** (`wind_factors`), the structures' local
  conditions and wind studies: factors on, or replacements of, the
  **pressure** only; the direction stays the case's (a local direction,
  e.g. a valley channelling the wind, is not modelled).

### The ruling-span method

A section solved by the ruling span has one load for all its spans. With
a wind at an angle it takes the wind across whose resultant per metre is
the **largest** of the spans' — each span's own normal part, vertical part
and friction (above) — so that the tension is at least the largest
span's: **Conservative**. The plane of the load and the parts along the
line are left out (**Simplification**: the finite elements give them).
The sag-tension job (`ohl sagtension`), which knows no line direction,
takes a case with a direction as across (the full pressure,
**Conservative**) and says so.

### What the span load leaves out

- **The chord for the tangent.** One load per span on the direction of its
  3D chord between its clamps as strung. **Simplification:** the wire is
  steeper than its chord near the supports and swung by the wind; for a
  wind at an angle its normal part changes by a few percent towards the
  ends, either way. Exact: each element its own direction from the
  previous pass of the stretch fixed point (planned, not built).
- **The chords as strung.** A suspension clamp at an angle structure hangs
  into the angle; its spans' chords are then a little off their route
  segments, so a wind "along the line" still gives them a small normal
  part (under 1 % of the across load in the tests' 20° angle).
- **Small angles:** see the cross-flow law above.

## The structure bodies

A load case's wind on the structure bodies (wind bands, the member wind
of the fe check, poles, OHL-Structure's line cases) takes, in order:

1. `[case.structure] direction` when given — in the **structure's axes**
   (x to its right, y ahead), the program's convention (version 7) or the
   old one (files up to 6, kept as read);
2. else the wires' direction at an angle (the load case's
   `wind_direction`, else the weather case's), relative to the line at
   the structure; for a structure **turned off** the line's direction (its
   `orientation_deg` not on the bisector) converted to its axes:
   \\( \theta_s = \theta + (\varphi_s - \varphi_l) \\), the angle between
   the structure's and the line's directions ahead;
3. else across the line from the wires' side, as before.

The member wind, the pole and the wind bands for that direction:
[Wind on structures › The wind's direction](structure-wind.md#the-winds-direction)
(the force on an inclined member is described there; a wind band holds
one area, said **not on the safe side** for a wind with a part along the
line).

## Load cases: one direction, or a set

- `wind_direction` (load case file version 7) replaces the weather case's
  direction for the wires and the bodies; across the line it is the across
  rule from that side (as `wind_from`, still read).
- `wind_directions = [0, 45, …]`: the case is solved **once per
  direction**, each as its own case named `<case> (wind from θ°)`; the
  structure checks take each as its own load and keep the worst, naming
  the case (and so the direction) that governs; the structures' groups
  know them by the set's name; OHL-Structure gets one case per direction.
  Cost: one line solve per direction.
- Every result of a case with a direction says *one direction, not the
  across envelope*: a wind across the line is not checked by that case.
  The across rule stays the default; a direction is always the user's or a
  design-code plugin's.

## Clearances and the views

**The views** (2026-11-16, the owner's decision: "We should just display
both sides at once"): every case with wind is drawn **as it is** — a
case **without** a direction across from the right (90°), a case that
**gives** its direction (across the line, 90° or 270°, or at an angle)
as given — **and mirrored about the line**
(\\( \theta \to 360° - \theta \\); across: the other side). Once only
when the mirror is the same wind (along the line: 0°, 180°) or another
case drawn with it is its mirror (e.g. the two copies of one source
wind, from 90° and from 270°, shown together). There is no side to
choose any more (until 2026-11-16: "as each case", "from the left",
"both"). The case as it is is the one reported, checked first and loaded.
A case with its own direction keeps it (2026-11-15): never turned to a
side; two cases differing only in their direction are two winds, never
one.

**The clearance checks** take the same two sides. Their "both sides"
(the default) for a case without a direction: across from the left and
from the right; for a case with its own direction: the direction and its
mirror — across the line the two sides, at an angle θ the winds from θ
and from 360° − θ, along the line (0°, 180°) the direction once. One
side only: a case without a direction from that side; a case with its
own direction, the direction alone — never turned to the side asked for,
which would check another wind than the case's (owner, 2026-11-15,
question 15).

- The vertical and radial clearance windows, `ohl zones` and the wire to
  wire swing arc (`ohl_clearance::zone::wind_states`): as above since
  2026-11-15; along the line once since 2026-11-16 (before, the same
  position twice — the same result, counted twice).
- The danger trees and grow-in, the safety volumes, `ohl analysis` and
  the distance between conductors under unequal wind
  (`ohl_clearance::positions::from_side`): until 2026-11-16 the two
  sides of a case **at an angle** were both the case as it is — the side
  was set, which a wind at an angle does not read — so its mirror was
  **never checked** (not on the safe side against the views, which drew
  it). Now from the right the wind from θ when it comes from the right
  half (0°–180°), else its mirror, and from the left the other one: each
  blowout blows the wires to the side named; along the line one blowout.
  Cases without a direction or with their own across the line: unchanged,
  bit for bit.

**The swing arc** between still air and the two blowouts
([Vertical and radial clearances](clearances.md#wind-positions),
[LiDAR analyses](lidar-analysis.md#wire-positions-checked)) is built from
the solved positions whatever their wind: for a wind at an angle from
the blowouts at θ and 360° − θ, along the line from still air to its one
blowout (no sideways swing — that is the case as given).

- **Assumption (the case's, not an envelope):** a case with its own
  direction checks that wind and its mirror only. A wind at an angle
  swings the wires less across the line than the across rule at the same
  pressure (the part normal to each span), and along the line hardly at
  all: a wind across the line is **not** checked by such a case. The
  across rule — every span the full pressure square to itself — stays
  the default and is the envelope over the horizontal directions at a
  pressure; a direction is always the user's or a design code's.
- **Conservative (as for the across rule):** the band runs from the
  smallest to the largest angle and radius of the three positions and is
  widened by its margin (0.15 m by default). A wind at an angle also
  moves the strings along the line; the blowouts hold that (solved), the
  positions between them lie within the band and its margin — measured
  2026-11-16 on the example line, both windy cases from 30°, 60° and 0°
  against their steps of 10 Pa from both sides (3960 positions): a
  step's centre line lies up to 0.082 m outside the band without its
  margin (30°: 0.042 m, 60°: 0.082 m, 0°: 0.001 m), against 0.091 m for
  the same cases across the line — held by the margin
  (`crates/ohl-clearance/tests/arc_band.rs`). Exact at the pressures
  solved: the wind steps.

## Validation

`winddir-001…008` (`benchmarks/weather/wind_direction.toml`): the force per
metre against the hand formulas — 15 N/m and 4500 N square to the wire,
10.61 N/m and 3182 N at 45° (projected), 7.5 N/m and 2250 N (cross-flow),
nothing along the wire, 3.75 N/m normal and 3.25 N/m vertical on a span
rising 30° (down uphill, up downhill), the friction along the wire.

Unit and solve tests (`crates/ohl-weather/src/direction.rs`,
`crates/ohl-line/src/solve/wind_direction_tests.rs`,
`crates/ohl-loads/src/direction_tests.rs`): the convention and the
conversion of old angles; the plan's hand figures (4500 / 3182 / 2250 N on
the 300 m span, the support reactions equal to \\( qD\sin\theta \\) or
\\( qD\sin^2\theta \\) times the wire's length to 0.1 %); the friction
(\\( c_t\,qD \\) per metre from ahead, \\( c_t\,qD/2 \\) at 45°); the
vertical part on a span rising 30° (3.25 N/m, down uphill, up downhill, to
0.2 %); a 20° angle structure (the supports' sum equal to each span's own
normal part on its route segment, to 0.5 %); a section entered against
the line; 90° and 270° bit for bit the old right and left; a set of
directions equal to its cases solved apart, bit for bit; old load case
files' angles; a turned structure's axes; and the example lines' full
outputs identical to the program before the change.
