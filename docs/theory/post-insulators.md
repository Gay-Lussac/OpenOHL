# Post insulators

*Built, in active development (not approved yet); owner, 2026-11-14:
"put post insulators on structures … so that we can make connections to
a rigid piece".* Implemented in `crates/ohl-line` (`assembly.rs`,
`solve.rs`, `jumper.rs`) and `crates/ohl-loads`. A **post insulator**
stands on a point of the structure — its **base**, the earthed end — and
holds a wire point at its other end, its **top**, as a cantilever: it
takes the load across it in bending, not as a string that swings. It is
placed with the **Add post insulator** tool, or given as a string with
`post` (line file version 17), or a string naming a library post (`kind = "post"`, insulator file version 6): an upright
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

- Posts on the ground and a rigid conductor carried on posts — the
  [plan](../development/decisions.md#2026-11-14). Posts are placed with
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
library's round trip.
