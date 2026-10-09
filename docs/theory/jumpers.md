# Jumpers

Implemented in `crates/ols-line` (`jumper.rs`, `rod.rs`). A jumper is a short slack cable joining two attachment points: the ends of two sections at a dead-end structure, or a tap from a branch to a through wire. The line file fields are described in [Line files](../user-guide/line-file.md#jumpers).

A jumper's pull, tens to a few hundred newtons, is negligible next to a section's tension, so **sections are solved first** and each jumper is then solved on its own between the sections' solved clamps. The steps, in every weather case:

1. **Ends.** Each end is the solved clamp of the section through that attachment, so the jumper follows strain assemblies as they settle and suspension clamps as they swing. If no section uses the attachment, the end is the attachment's wire point.
2. **Length.** The jumper's unstressed length at its temperature \\( t_0 \\) is the given `length`, or is **cut** from the given `sag` (below). In a case at temperature \\( t \\) it is \\( L_0 = L_{cut}\, e^{\alpha (t - t_0)} \\), with the cable's thermal expansion \\( \alpha \\). The jumper therefore lengthens when hot and shortens when cold, and its sag follows the weather like a wire's.
3. **Load.** The case's weight, ice and wind per metre. The wind is horizontal and perpendicular to the jumper's chord, on the iced diameter. In a bundle, each subconductor carries its own load and a share of the support string's weight.
4. **Shape.** Solved with one of the two models below.

## Hinged jumper: one catenary element

If neither end is clamped (both ends are taps on through wires), or `bending_stiffness` is 0, the jumper is one **exact elastic catenary element** between its ends. This is the element of [Finite-element wire systems](wirefe.md): it is exact for any sag and needs no meshing. With a support string, the jumper is two elements and a tension-only truss, and the Newton solver of `ols-wirefe` finds the point where the string holds it.

## Clamped jumper: an elastic rod

Where an end is the first or last point of a section, the clamp holds the jumper. It leaves along the conductor's axis there (the span's tangent at the clamp, continued), turned down by `angle_from_deg` / `angle_to_deg` (default 30°). The cable's bending stiffness \\( EI \\) carries that direction into the jumper. The jumper can therefore not swing in the wind as if hinged.

The model is an **inextensible elastic rod**:

- **Pieces.** \\( n \\) straight pieces of equal length \\( \ell \approx 0.12 \\) m, with \\( 16 \le n \le 40 \\). The unknowns are the pieces' unit directions \\( d_0 \dots d_{n-1} \\). Node \\( m \\) is at \\( A + \ell \sum_{i<m} d_i \\).
- **Energy.** The load's potential, plus a bending term \\( c\,(1 - d_i \cdot d_{i+1}) \\) at every inner node and \\( 2c\,(1 - d \cdot e) \\) at a clamped end (\\( e \\): the direction the clamp holds), with \\( c = EI/\ell \\).
- **Constraints.** The far end is at \\( B \\): \\( \ell \sum d_i = B - A \\). A taut support string keeps its node at the string's length from its top.
- **Solution.** Newton's method on the stationarity of the Lagrangian. Each direction is rotated within its normal plane, so the pieces keep their exact length and short stiff pieces never stretch artificially. Levenberg–Marquardt damping and a backtracking line search keep the steps safe.
  - **Start:** the hinged catenary under the same load.
  - **If that doesn't converge:** \\( EI \\) is brought in in steps (0, 5 %, 20 %, 50 %, 100 %), each step starting from the last.
  - **With a support string:** the string is first left out, then shortened to its length in steps.
- **Tolerance.** Moments to \\( 10^{-9} \\) of the load and bending scale; closure gap to \\( 10^{-10} \\) of the length. The solve runs in coordinates relative to \\( A \\): in project coordinates (~10⁶ m) rounding would swamp these tolerances.
- **Exit clauses.** 150 iterations and 60 s per solve. The error states the residual as a multiple of the tolerance.
- **Stretch.** The cable's stretch is left out, because it is ~10⁻⁶ at a jumper's tension.

**Default \\( EI \\).** Without a `bending_stiffness`, \\( EI \\) is estimated from the cable as \\( EA\,(D/6)^2/16 \\). This assumes the strands bend each on its own, with a strand diameter of \\( D/6 \\). For Drake 26/7 it gives ~48 N·m².

## Cutting a jumper given by its sag

A `sag` is the jumper's largest vertical distance below its end-to-end chord, in still air at its temperature (its **design case**). The cut length is found in that design case:

1. The sections at the jumper's ends are solved in the design case, which gives the clamps' positions there.
2. **Hinged jumper:** the length is found by bisection on the sag–length relation of the catenary element (~50 element solves).
3. **Clamped jumper:** that hinged length is the starting bracket. The Illinois method then finds the rod length that gives the sag (5–15 rod solves).

**Kept until something changes.** The cut is kept, in memory, for every weather case. It is cut again only when one of these changes:

- the jumper itself (its sag, temperature, cable, ends, angles, \\( EI \\), support);
- the sections at its ends;
- the structures they or the jumper hang on;
- the cables;
- the line's creep-stretch and bimetallic settings.

All of these form the key the cut is kept under, so a change to any of them recuts the jumper. Each case still solves the jumper's shape from its own hanging catenary. Starting instead from the design-case shape was measured on a 433-jumper line: about 6 % faster per case, at the cost of one more solve per jumper when the project opens. It was left out.

## After an edit

An edit re-solves its own sections. A jumper is solved again only if one of these changed: the jumper itself, a section at its ends (or such a section was re-solved), or a structure it hangs on. Every other jumper keeps its solution. The result is the same as solving everything again; the tests check this after each kind of edit.

## When a jumper fails

- **Taut.** In wind, free strain strings can swing their clamps further apart than the jumper is long. The jumper's pull holding them back is not modelled, because sections are solved first. That jumper is reported as *taut* in that weather case, instead of being drawn at hundreds of kN.
- **Ends less than 0.1 m apart horizontally.** A vertical jumper is not supported yet.
- **No convergence.** The message names the residual. This is rare since the solve runs in coordinates relative to the first end.
- **Sag cannot be reached.** The sag asked for is less than the taut jumper's, or would need more than 20× the chord length.

## Expected ranges

Physical values for typical high-voltage jumpers (indications to check a model against, not limits):

| Quantity | Typical range | Notes |
|---|---|---|
| Length | 3–15 m | longer on wide dead-end crossarms and taps |
| Sag in still air | 0.5–3 m | clearance to the structure usually decides it |
| Tension | 50–1 000 N | tens of newtons for a short light jumper, hundreds with ice or wind |
| \\( EI \\) (estimated) | 2–200 N·m² | grows with \\( EA\,D^2 \\): ~2 for a 100 mm² wire, ~48 for Drake 26/7, ~150 for a 40 mm conductor |
| Clamp exit angle | 0–45° below the conductor's axis | default 30°; 0: straight on |
| Change of sag between −20 °C and 80 °C | a few cm to a few dm | the length changes by \\( \alpha \Delta t \approx 2 \cdot 10^{-3} \\); the clamps move too, so the sag below the chord can even shrink when hot |

Computation (release build, one core):

| Step | Expected cost | Exit clause |
|---|---|---|
| Catenary element solve | ~1 µs | 100 iterations, 20 ms |
| Rod Newton iteration (\\( 2n+3 \\) unknowns, \\( n \approx 24 \\)) | ~50 µs | — |
| Rod solve (5–30 iterations) | 0.25–1.5 ms; ~0.75 ms on average | 150 iterations, 60 s |
| Rod solve with a support string | a few times the above | string steps no smaller than 1/64 of the way |
| Cut from a sag, hinged | ~50 µs | 200 iterations; 20× chord |
| Cut from a sag, clamped | 5–15 ms (end sections plus 5–15 rod solves) | 40 iterations; 20× chord |
| A weather case, 433 clamped jumpers | ~0.3 s | — |
| An edit of one section (its jumpers only) | ~10 ms | — |

A weather case is solved in full the first time it is shown, which costs the line's jumpers once; after that only edits re-solve them.
