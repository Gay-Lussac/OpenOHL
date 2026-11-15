# Post insulators

*Built, in active development (not approved yet); owner, 2026-11-14:
"put post insulators on structures … so that we can make connections to
a rigid piece".* Implemented in `crates/ohl-line` (`assembly.rs`,
`solve.rs`, `jumper.rs`) and `crates/ohl-loads`. A **post insulator**
stands on a point of the structure — its **base**, the earthed end — and
holds a wire point at its other end, its **top**, as a cantilever: it
takes the load across it in bending, not as a string that swings. It is
placed with the **Add post insulator** tool, or given as a string with
`post` (line file version 17), or a string naming a library post (`kind = "post"`, insulator file version 6), or a structure type's string with `post` (structure file version 13, [below](#in-ohl-structure-the-moment-on-the-pylon)): an upright
station post (`angle_deg` 180), a horizontal line post (90), or any
direction between.

## In the wires' solve: a fixed point

The post is taken as **rigid**: its top does not move. The wire point it
holds is fixed to the structure where the post puts it (its base, its
length, its direction), as a wire clamped to the structure: the section's
node there is a fixed node, and its reaction is the post's load. A post
holds its wire point alone (another string at the same wire point is
refused: a braced post is a hinged one, below) and is never a strain
string.

**Jumpers on its clamp.** A jumper ending at the post's top puts its
load on a clamp fixed to the structure: it goes straight into the post's
load, as the jumpers' loads on a hanging clamp go into its strings — the
estimated load (half the jumper's weight, ice and wind) or the jumper's
own pull, as the line's calculation options say. Where no section ends
at the post's top (a jumper from a dead end to the post), the jumper's
end load is the post's alone. *(Before 2026-11-14 a jumper's load on any
clamp fixed to the structure — a post's top, a wire point without
strings — was left out of the structure loads: not on the safe side.)*

**Simplification:** a real post bends under its load: its top moves by
its deflection — millimetres to a few centimetres at its rated load, from
its data sheet. Rigid, it is **on the safe side for the post's own load**
(a support that does not give way takes all the unbalanced pull of the
spans) but **not for the clearances**: the conductor at the top may sit
up to the post's deflection farther than computed. Exact: the post as a
spring of its deflection (a lateral stiffness \\( 3EI/L^3 \\) from the
data sheet), not yet modelled.

## Its load on the structure: the moment of its height

The force \\( \mathbf F \\) the wires put on the post's top acts at the
top, a lever \\( \mathbf r \\) (top − base) away from the base point. The
structure takes it at the base as the same force and the moment

\\[ \mathbf M = \mathbf r \times \mathbf F \\]

— for an upright post of height \\( h \\) and a load \\( F_\perp \\)
across it, \\( M = h\,F_\perp \\), the post's bending moment at its base
(the quantity its cantilever rating is given for, at its top). The load is
reported as the base point's, its lever kept (`AttachmentLoad::lever`,
labelled "A1 (post, 1.60 m)"), and its moment summed per point with the
factored forces. The structure analysis puts the moment at the base
point's joint where a frame member meets it. **NOT ON THE SAFE SIDE
where it cannot**: at a joint only trusses meet, and in the pole check
(forces at points only), the moment is left out and the result says so.

**Its own weight and wind**, at its middle (lever \\( \mathbf r / 2 \\)):
its weight, and the wind on its wind area — the line's, across the line,
times the structure's local wind factor, as on the strings. **Ice on the
post is not counted**, as on the insulator strings: a simplification,
small next to the wires' ice, not on the safe side. Exact: the ice on
the post's sheds.

## In OHL-Structure: the moment on the pylon

*2026-11-14.* A fixed post's moment reaches **OHL-Structure** too: the
line sends, per point and case, the moment of each force component (across,
along, vertical) at the post's lever and the jumpers' moment on its clamp
(the line-values file, version 8); a structure load case's line forces take
them with **their own factors** — the moment of the transverse force times
the transverse factor, and so on, the clamp's moment times the largest
factor — so the factored moment is exact, not estimated. The finite-element
check puts it at the post's base joint **where a frame member meets it**;
where only trusses meet, it is left out and the result says **NOT ON THE
SAFE SIDE**. The pole check takes forces at points only and says the same.
A **hinged** (braced) post sends no moment: its force and its brace's reach
their points as plain forces. A share of the spans' pull (back / ahead below
100 %) does not scale the post's moment: from the whole pulls, on the safe
side for shares of 100 % or less (said on the row).

**On a structure type** (*2026-11-15*). A structure type's string can be
a post with the same fields (`post`, `hinged`, `cantilever_load`,
`exit_angle`, `exit_direction`; structure file version 13, [Post
insulators on a type](../user-guide/structure-library.md#post-insulators-on-a-type)).
Resolving the line puts them on the line's strings of every structure of
the type: the post is then the same object as one made by hand, and
everything on this page applies to it unchanged — the rigid fixed point,
the moment of its height, its own weight and wind, the jumper's clamp, the
checks of its ratings and the moment sent to OHL-Structure, with the same
NOT ON THE SAFE SIDE notes where they apply. No simplification is added:
the tests compare a typed post's loads and line values with a hand-made
one's. The type is refused where the line would refuse its strings (a
fixed post sharing its wire point, a hinged post without a brace, a strain
post, a post standing on a wire point). OHL-Structure edits a type's posts
(fixed or hinged, rating, exit angles) and draws them: a fixed post with
its base plate, a hinged post with its hinge and its brace.

**A fault fixed on the way:** at a post's base the line has two loads —
what acts at the post's top, and the post's own weight and wind at its
middle. OHL-Structure read only the first of them: forces were missing (not
on the safe side) between 2026-11-14's first step and this one. The values
are now summed per point.

## A jumper at its top: a fixed exit angle

The clamp on a post's top holds a jumper (or a conductor's end) **in a
direction fixed to the structure** (owner: "on the post the flexible
should come in and have a fixed exit angle"): `exit_angle_deg` from the
horizontal (none: level) and `exit_direction_deg` in plan in the
structure's axes (none: towards the jumper's other end as given). Unlike a
section's end clamp, it does not turn with the wind. The jumper is solved
as the elastic rod with that end clamped
([Jumpers](jumpers.md#clamped-jumper-an-elastic-rod)): its bending stiffness
carries the direction into it.

**Its moment on the clamp.** Held in a direction \( \mathbf e \), the
rod's end piece (length \( \ell \), direction \( \mathbf d \)) stores
the energy \( (2EI/\ell)(1 - \mathbf d \cdot \mathbf e) \); turning the
clamp by a small rotation \( \boldsymbol\omega \) changes it by
\( -\boldsymbol\omega \cdot (2EI/\ell)\, \mathbf e \times \mathbf d \), so
the jumper's moment on the clamp is

\[ \mathbf C = \frac{2EI}{\ell}\, \mathbf e \times \mathbf d \]

(the whole bundle's: times the subconductors). It is added to the post's
base moment, \( \mathbf M = \mathbf r \times \mathbf F + \mathbf C \), in
the structure loads (a moment of its own at the base point,
`AttachmentLoad::couple`) and in the post's check. **Simplification, on
the safe side:** factored by the largest of the load case's three factors
(the moment comes from the jumper's weight, ice and wind together).

**NOT ON THE SAFE SIDE with the estimated jumper loads** (the line's
default): the jumper's end *force* on the clamp is then the estimate the
sections take (half its weight, ice and wind), while the moment is the
rod's — and the clamp's moment shifts the jumper's load between its ends,
so the two are not one state, and the error can go either way. The
structure loads and the string check say so, naming the jumpers. Exact:
the jumpers' own pull (calculation options, `jumper_loads = "pull"`):
force and moment both from the solved rod. A jumper given no bending
stiffness (`bending_stiffness` 0) is hinged: no moment. On a **hinged**
post's clamp the jumper's moment is not taken (the post's bending from it
is not checked).

## A braced post: hinged

A **braced line post** stands on a **hinged base** (`hinged`): it turns
about its base, so it carries its load along it only — it pushes or
pulls, a two-force member — and another string at its wire point, its
**brace**, holds it in place (a hinged post without one is refused: it
would fall over). The post and its brace are then an assembly like a V:
its wire point moves with them in the solve, each member's force as
solved (tension positive), the post a member that may push (its
compression rating, when given, flags it beyond; else unlimited). Its own
weight and wind are those of a string: half at each end. With the brace
at 45° above a level post, the brace carries the load on the clamp
divided by \( \sin 45° \) and the post pushes with its horizontal part —
the statics the test checks.

### A pivoting post: held by what is attached to it

*Owner, 2026-11-15: "Pivoting should simply be tied to the jumper or
piece it's attached to. And while nothing is attached to it it should just
move freely. It's not the best solution but it should represent the
truth. If braced then some degrees of freedom are locked."*

The post and its braces are pinned at both ends: the post turns about its
hinge in **every direction its braces leave free**, and nothing in the
assembly gives it a stiffness there. With **one brace** (or braces whose
bases lie on one line with the post's base, e.g. straight above it) the
pair turns freely about the line through their bases — for the usual
braced line post, the vertical through its base: its top can move along
the circle of its length, along the line. With two braces off that line
(one ahead and one back) every direction is locked.

**What holds it is what is attached to its top.**

- **The conductor** (a section ends at its top): its top is a free node of
  the section's solve, held by the post and the brace as bars (each its
  axial stiffness, the post pushing as well as pulling) and by the spans
  clamped there. Along the free direction only the wire holds it: the post
  **swings along the line until the wires' pulls balance**, as a
  suspension string swings — at the solved top the wires' pull has no part
  along the circle,
  \( (\mathbf F_{back} + \mathbf F_{ahead}) \cdot \mathbf t = 0 \)
  (\( \mathbf t \) the circle's tangent; the members' weights act down,
  across it for a vertical axis). The spans' tension difference is relieved
  by that swing: a post free to turn about a vertical line takes along the
  line only what its swing turns of the spans' pull across the line
  \( F_\perp \): \( H_{ahead} - H_{back} \approx F_\perp \tan\varphi \). The
  test's hand check: two level 300 m spans clipped in with 300.70 m and
  300.40 m of wire, a 1.5 m level post braced from 1.5 m above its base:
  the angle at which the two elastic catenaries' pulls on the top at
  \( (1.5 \sin\varphi, -1.5\cos\varphi) \) have no part along the circle,
  found by bisection — \( \varphi = 5.70° \), the top 14.9 cm along the
  line, the spans' \( H \) then within 0.1 N; the brace's force from the
  vertical balance, the post's from the horizontal one: the solve agrees
  to 1 µm and 10⁻⁶.
- **In the blocks** (a section strung by tension rolls over sheaves in
  every weather case, [Wire finite elements](wirefe.md)): a suspension
  string swings to the sheave's resultant there. A pivoting post has no
  such position of its own — solved alone under the sheave's pull it
  turns without end (before 2026-11-15 the rolling then did not settle and
  the section was reported not solved). While the length is shared out
  over the sheaves the post is taken where it is given to point (as the
  crew clamps it); the swing comes in the solve that follows, where the
  clamped wire holds it. **Simplification**: the sheave on its top shares
  the length as at its given position, not at its swung one (millimetres
  to centimetres along the line); exact: the post's swing in the sharing.
  Not shown to be on either side; the swing in the solve is exact for the
  lengths shared.
- **A jumper only** (no section ends at its top): the post and its brace
  are solved alone under the jumper's pull on the clamp
  (`post::hinged_alone`): turned about its free line until the pull's part
  across that line points away from it — the post hangs in the load as a
  pendulum, the stable one of the two balances; a load along the line
  only turns it nowhere (every turn balances; it stays as given) — then
  the members' forces from statics, each on its base with half its weight.
  Where the pull points back towards the hinge (a balance that does not
  hold) nothing near its given position holds it: its loads are not
  computed and the case says so. **Simplification, NOT ON THE SAFE SIDE
  for the directions** (said with every case): the jumper is solved to the
  post's top where given, not again with the post swung; a small push
  along the line turns a free post a long way (in the test the jumper's
  28 N of wind along the line turns it 90°). The sizes of the post's and
  the brace's forces stand (the same in every turn about a vertical line);
  their directions in plan do not where it swings far.
- **Nothing**: free. No load path — no stiffness is made up for it; its
  loads (its own and its brace's weight and wind) are not computed, and
  every load case says so (*hinged post P at T: nothing holds it (free to
  pivot); its loads not computed — NOT ON THE SAFE SIDE*), as do the
  string checks (not checked).

**The hinge is taken frictionless** (the owner's free pivot). **NOT ON THE
SAFE SIDE for the structure's longitudinal load**, said with every load
case for each pivoting post a section ends at: free, the post passes
along the line only what its swing turns of the pull across it, and the
spans' pull difference stays in the wire; a hinge that sticks (friction,
corrosion, ice) holds the post where it is and passes part of that
difference into the structure, as a fixed post passes all of it. Exact: a
hinge with its friction moment. The post's and brace's own forces are
those of the swung geometry, a little different from the unswung one.

**Its loads on the structure**: the post's axial force at its hinge and
each brace's at its base, each with half its weight — as for strings
(`SectionShape::attachment_loads`); they reach the structure loads and
the line values sent to OHL-Structure as plain forces at those points (no
moment: the hinge passes none).

## Its ratings checked

**Checks › String checks…** (a rule with `load`) checks each fixed post
in each weather case against its own ratings, where given (each is an
optional limit: unchecked when empty, the notes count the posts without
any):

- **Cantilever load.** The data sheet's rating is the load across the
  post at its top it withstands; the post fails in bending at its base.
  The check takes the bending moment at the base — the wires' force at
  the top (sections and jumpers), the post's own wind and weight at its
  middle, the jumpers' moment on its clamp,
  \( \mathbf M = \mathbf r \times \mathbf F + \tfrac12 \mathbf r \times
  \mathbf F_{own} + \mathbf C \) — its part across the axis
  \( M_\perp \), and compares the **equivalent load at the top**
  \( M_\perp / h \) with the rating.
- **Compression** and **tension** along it (`compression`, `max_load`):
  the force along its axis, the worse of its top (the wires alone) and its
  base (with its own weight).
- The twist about its axis (from the clamp's moment) is computed but not
  checked: posts are given no torsion rating.

A hinged post is checked from its solved force: its compression and
tension against its ratings; no uplift row (it may push). The loads are
the weather case's, unfactored, against the ratings as given — **no
strength factor is applied**: a design code's factor on the ratings (from
its plugin) is not built in, as for the strings' maximum load.

## Not yet

- A rigid conductor carried on posts — the
  [plan](../development/decisions.md#2026-11-14).
- A pedestal's own loads: a post on the ground stands on a **post
  support** ([Add post insulator](../user-guide/tools-structures.md#a-post-on-the-ground-a-post-support)),
  a structure without a type whose pedestal is a point — its loads on the
  ground lack the pedestal's weight and wind (said NOT ON THE SAFE SIDE
  for its foundation in the window). A pedestal structure type would
  carry them. Posts are placed with
  the [Add post insulator](../user-guide/tools-structures.md#add-post-insulator)
  tool (2026-11-14).
- The post's deflection (a spring at its top) and its own ice.

*Validation:* `ohl-loads` `post_tests` (the load at the top with its
lever, the moment, the post's own weight and wind at its middle, the
clamp fixed; a jumper on the post's top reaching its base, its far end
on a wire point no section ends at, the clamp's moment in the structure
loads; a braced post pushing and its brace pulling against statics),
`ohl-line` `a_post_holds_the_jumper_in_a_fixed_direction` and
`post::tests` (the split into bending, torsion, tension and compression),
`ohl-clearance` `a_post_is_checked_against_its_ratings`, the insulator
library's round trip; on a structure type: `ohl-structures`
`version_13_post_strings`, `ohl-line` `a_type_strings_posts_reach_the_line`,
`ohl-loads` `a_typed_post_loads_as_one_made_by_hand`, `ohl-structure-tests`
`a_typed_posts_moment_reaches_the_pylon` and
`a_typed_hinged_post_sends_forces_only`; a pivoting post: `ohl-line`
`pivot_tests` (one brace pivots, two do not; the swing against the hand
check; in the blocks over sheaves and with equal H, an inclined span, wind,
a weight in one span: settled and balanced along the circle) and
`post::tests` (alone under a load: the swing atan(300 / 400) and the
statics; nothing attached said), `ohl-loads`
`a_jumper_holds_a_pivoting_post`, `ohl-structure-tests`
`a_typed_post_with_one_brace_pivots_and_sends_forces` (the example line in
every weather case); benchmark `tbins-006` (`benchmarks/wirefe/textbook-insulators.toml`:
the swing, the spans' tensions and the members' forces against an
independent reference — quadrature catenaries and Newton on the clamp,
`benchmarks/reference/textbook_reference.py pivot` — to 10⁻⁷).
