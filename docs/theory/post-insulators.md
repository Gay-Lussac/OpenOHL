# Post insulators

*Built, in active development (not approved yet); owner, 2026-11-14:
"put post insulators on structures … so that we can make connections to
a rigid piece".* Implemented in `crates/ohl-line` (`assembly.rs`,
`solve.rs`, `jumper.rs`) and `crates/ohl-loads`. A **post insulator**
stands on a point of the structure — its **base**, the earthed end — and
holds a wire point at its other end, its **top**, as a cantilever: it
takes the load across it in bending, not as a string that swings. It is
given as a string with `post` (line file version 17), or a string naming a
library post (`kind = "post"`, insulator file version 6): an upright
station post (`angle_deg` 180), a horizontal line post (90), or any
direction between.

## In the wires' solve: a fixed point

The post is taken as **rigid**: its top does not move. The wire point it
holds is fixed to the structure where the post puts it (its base, its
length, its direction), as a wire clamped to the structure: the section's
node there is a fixed node, and its reaction is the post's load. A post
holds its wire point alone (another string at the same wire point is
refused: braced posts are not modelled yet) and is never a strain string.

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
carries the direction into it. **Not yet:** the moment the jumper puts on
the clamp (a few N·m) added to the post's base moment.

## Not yet

- The post's ratings checked (its cantilever load across it at its top,
  compression and tension along it); they are kept with the post.
- Braced (hinged) posts, posts on the ground, an Add post tool, a rigid
  conductor carried on posts — the [plan](../development/decisions.md#2026-11-14).

*Validation:* `ohl-loads` `post_tests` (the load at the top with its
lever, the moment, the post's own weight and wind at its middle, the
clamp fixed), `ohl-line` `a_post_holds_the_jumper_in_a_fixed_direction`,
the insulator library's round trip.
