# Jumpers

Implemented in `crates/ols-line` (`jumper.rs`, `rod.rs`). A jumper is a short slack cable joining two attachment points: the ends of two sections at a dead-end structure, or a tap from a branch to a through wire. The line file fields are described in [Line files](../user-guide/line-file.md#jumpers).

**Its weight on the clamps.** The jumper's **weight** (and ice) does reach the sections: in each weather case, the clamps at its ends carry the part of it between each end and its first support — half of it without supports — as a load hanging at the clamp (`jumper::end_weights`). Its length for that is its cut length, else the length giving its sag between the wire points in still air. It is not there at stringing (jumpers are hung after the wires are strung), so a section with jumpers solved at its stringing temperature carries a little more tension than it was strung to. A section is solved again when the jumpers on it change.

A jumper's pull, tens to a few hundred newtons, is negligible next to a section's tension, so **sections are solved first** and each jumper is then solved on its own between the sections' solved clamps. The steps, in every weather case:

1. **Ends.** Each end is the solved clamp of the section through that attachment, so the jumper follows strain assemblies as they settle and suspension clamps as they swing. If no section uses the attachment, the end is the attachment's wire point.
2. **Length.** The jumper's unstressed length at its temperature \\( t_0 \\) is the given `length`, or is **cut** from the given `sag` (below). In a case at temperature \\( t \\) it is \\( L_0 = L_{cut}\\, e^{\varepsilon(t - t_0)} \\), with the cable's thermal strain without tension \\( \varepsilon \\), the same as the line's cables have (below). The jumper therefore lengthens when hot and shortens when cold, and its sag follows the weather like a wire's.
3. **Load.** The case's weight, ice and wind per metre. The wind is the line's: horizontal, across the line from the case's side (left or right looking ahead along the routes), whichever way the jumper was entered. The jumper takes the part normal to itself, by \\( \cos^2 \\) of the angle between the wind and its normal, on the iced diameter. In a bundle, each subconductor carries its own load and a share of the support string's weight.
4. **Shape.** Solved with one of the two models below.

## The same cable model as the spans

A jumper uses the cable as the line's spans do, as far as it matters at a jumper's tension:

- **Thermal behaviour, bimetallic cables included.** At \\( t_0 \\) the jumper's components are taken as unstressed. Without tension, both components share one strain and their forces balance, which gives the composite expansion \\( \varepsilon = \bar\alpha\,\Delta t \\) with \\( \bar\alpha = \sum E_iA_i\alpha_i / \sum E_iA_i \\). In a bimetallic cable warmer than \\( t_0 \\), the outer strands, which expand more, are pushed into compression. The line's **bimetallic conductor model** (Criteria › Bimetallic conductor model, or the cable's own) limits that compression, as it does for the spans. Past the limit the outer strands carry only the limit force \\( F_{lim} \\) and the core sets the length: \\( \varepsilon = \alpha_c\,\Delta t + F_{lim}/E_cA_c \\). As a **bird cage** (no compression) the warmer jumper lengthens like its core alone. With the default compression limit it keeps the composite expansion. The limit is the one the spans use: the bimetallic model's, or the outer curves' own `max_compression` when lower (a non-linear cable). Colder than \\( t_0 \\), the outer strands are in tension and the composite expansion holds.
- **Stretch, stress-strain curves and creep are left out.** A jumper's tension, tens to a few hundred newtons, strains it by ~10⁻⁶, against ~10⁻³ in a span. The non-linear curves and creep would change its length by micrometres while making each rod solve far costlier, so the rod is inextensible.

**Compared** on the example's Drake 26/7 tap jumper (sag 1.5 m at 15 °C; release build; the sections at its ends follow the same model):

| Case | Composite (default model) | Bird cage | Difference |
|---|---|---|---|
| −20 °C | L₀ 10.7929 m, sag 1.759 m | the same | none: colder, the composite expansion holds |
| 50 °C | L₀ 10.8072 m, sag 1.221 m | L₀ 10.8044 m, sag 1.215 m | −2.8 mm, −5 mm |
| 80 °C | L₀ 10.8133 m, sag 0.961 m | L₀ 10.8081 m, sag 0.949 m | −5.2 mm, −12 mm |
| Time per solve | ~5 ms | ~5 ms | none: the strain is a closed form |

The bimetallic model is used: it costs nothing and keeps jumpers consistent with their spans. It only makes a difference for two-component cables as a bird cage or with a low compression limit, and there by up to about a centimetre of sag.

## Hinged jumper: one catenary element

If neither end is clamped (both ends are taps on through wires), or `bending_stiffness` is 0, the jumper is one **exact elastic catenary element** between its ends. This is the element of [Finite-element wire systems](wirefe.md): it is exact for any sag and needs no meshing. With a support string, the jumper is two elements and a tension-only truss, and the Newton solver of `ols-wirefe` finds the point where the string holds it.

## Clamped jumper: an elastic rod

Where an end is the first or last point of a section, the clamp holds the jumper. It leaves along the conductor's axis there (the span's tangent at the clamp, continued), turned down by `angle_from_deg` / `angle_to_deg` (default 30°). The cable's bending stiffness \\( EI \\) carries that direction into the jumper. The jumper can therefore not swing in the wind as if hinged.

The model is an **inextensible elastic rod**:

- **Pieces.** \\( n \\) straight pieces of equal length \\( \ell \approx 0.12 \\) m, with \\( 16 \le n \le 40 \\). The unknowns are the pieces' unit directions \\( d_0 \dots d_{n-1} \\). Node \\( m \\) is at \\( A + \ell \sum_{i<m} d_i \\).
- **Energy.** The load's potential, plus a bending term \\( c\\,(1 - d_i \cdot d_{i+1}) \\) at every inner node and \\( 2c\\,(1 - d \cdot e) \\) at a clamped end (\\( e \\): the direction the clamp holds), with \\( c = EI/\ell \\).
- **Constraints.** The far end is at \\( B \\): \\( \ell \sum d_i = B - A \\). A taut support string keeps its node at the string's length from its top.
- **Solution.** Newton's method on the stationarity of the Lagrangian. Each direction is rotated within its normal plane, so the pieces keep their exact length and short stiff pieces never stretch artificially. Levenberg–Marquardt damping and a backtracking line search keep the steps safe.
  - **Start:** the hinged catenary under the same load.
  - **If that doesn't converge:** \\( EI \\) is brought in in steps (0, 5 %, 20 %, 50 %, 100 %), each step starting from the last.
  - **With a support string:** the string is first left out, then shortened to its length in steps.
- **Tolerance.** Moments to \\( 10^{-9} \\) of the load and bending scale; closure gap to \\( 10^{-10} \\) of the length. The solve runs in coordinates relative to \\( A \\): in project coordinates (~10⁶ m) rounding would swamp these tolerances.
- **Exit clauses.** 150 iterations and 60 s per solve. The error states the residual as a multiple of the tolerance.
- **Stretch.** The cable's stretch is left out, because it is ~10⁻⁶ at a jumper's tension.

## Support clamps: a rod with rigid pieces

A jumper can hang on **helping strings** between its ends (`supports`): insulator strings of the structure holding it like suspension strings hold the conductor on the line. Each clamp hangs on strings of a structure: one string (an I) or two strings to one wire point (a V). The clamp is free to swing and turn, so no angle is assumed for it. The cable runs straight through it: over the clamp's few centimetres it cannot bend, so the direction it leaves in is the direction it enters in.

A jumper with support clamps is always solved as the rod above, also with \\( EI = 0 \\) or hinged ends:

- **Pieces.** The pieces now have their own lengths \\( \ell_i \\); node \\( m \\) is at \\( A + \sum_{i<m} \ell_i d_i \\). Each clamp is **one piece** of its length (`clamp_length`, default 0.1 m). One straight piece has one direction, so the jumper is straight through the clamp by construction, whatever its stiffness. The stretches between the clamps and the ends are cut into pieces of equal length, 16 to 40 in all (~0.12 m), at least one each, in proportion to their lengths.
- **Bending.** \\( c_i = EI / \bar\ell \\) at the node between pieces \\( i \\) and \\( i+1 \\), with \\( \bar\ell \\) the mean of their lengths; \\( 2EI/\ell \\) at a clamped end, with that end piece's length. With equal pieces this is the rod above.
- **Where the strings hold.** At the clamp's middle: \\( p = x_k + \tfrac12 \ell_k d_k \\) for the clamp's piece \\( k \\). The strings therefore turn the clamp too: it settles where the cable's pulls at its two ends and the strings balance.
- **Strings.** Each string is a rigid, tension-only distance constraint from its fixed top (a structure point) to the clamp's middle: an I is a pendulum, a V two such constraints. A string is **slack** while the clamp is nearer its top than its length, or while its force would push. The solve keeps an **active set**: all strings are first left out; a string whose point then hangs beyond its length goes taut and is shortened to its length in steps (as a single support string is); a taut string whose force would push is let go; this repeats until no string changes. A V leg on the lee side of a strong wind goes slack this way. A string with a `compression` (it may push) is taut from the start and never let go: it holds the clamp at its length whichever way, its force negative when it pushes.
- **Loads on a clamp.** Half of each string's weight, half of the wind on each string (the weather case's pressure × the string's `wind_area`, horizontal across the line from the case's wind side, as on the line's suspension strings), and the wire point's own `weight` (a counterweight), shared by the subconductors. The clamp itself takes no wind (it is small).
- **Forces.** A string's tension is \\( T = -2 \mu L \\) from its multiplier \\( \mu \\) (constraint \\( |p - t|^2 = L^2 \\)); 0 when slack. The Jacobian of the system, with the strings' rows and columns, is checked against finite differences in the tests.
- **Limits.** A clamp's strings hang from structure points: a string below another string (a V with a string below, a TAK) does not hold a clamp yet. The strings' pull on the structure goes into the structure loads, at their tops; the jumper's pull on the sections does not (sections are solved first).

**Default \\( EI \\).** Without a `bending_stiffness`, \\( EI \\) is estimated from the cable between two limits:

- **Strands locked together** (a solid bar): \\( EI_{max} = E\\,\pi d^4/64 \\), with the composite modulus \\( E = \sum E_i A_i / \sum A_i \\) and the overall diameter \\( d \\).
- **Strands slipping**, each bending on its own: \\( EI_{min} = \sum_i n_i E_i\\,\pi d_i^4/64 \\) over the components, with \\( n_i \\) strands of diameter \\( d_i = \sqrt{4A_i/(\pi n_i)} \\). This needs the strand counts (`strands` in the [wire library](../user-guide/wire-library.md)).

A clamped jumper is nearly locked close to its clamps, where the hardware squeezes it, and nearly slipped in the middle of its loop. One uniform stiffness stands for both:

\\[ EI = \max\left(k \cdot EI_{max},\\; EI_{min}\right) \\]

The **reduction factor** \\( k \\) is the jumper's `stiffness_factor`, default 0.2; the usual range is 0.2–0.35. It is the only input the jumper's design needs. The lower it is, the further the jumper swings, so 0.2 is the conservative side for clearances to the structure. The floor \\( EI_{min} \\) applies only when the strand counts are given.

For Drake 26/7 (composite \\( E \\approx 73.9 \\) GPa, \\( d = 28.14 \\) mm, 26 × 4.44 mm aluminium and 7 × 3.46 mm steel):

| | \\( EI \\), N·m² |
|---|---|
| \\( EI_{max} \\) | 2 275 |
| \\( 0.35\\, EI_{max} \\) | 796 |
| \\( 0.2\\, EI_{max} \\) (default) | 455 |
| \\( EI_{min} \\) | 37 |

## Rigid jumpers

A jumper given `rigid` is a tube or bar: the straight member between its two solved clamps, in every weather case. It has no length or sag of its own and no thermal change. Its tension is not computed, because the members holding it carry it. It is drawn, and checked for clearances, along that straight line, and it can never be taut. The PLS-CADD jumpers marked rigid are read this way.

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

- **Taut.** The clamps end up at least as far apart as the jumper's unstressed length in that weather case: \\( |B - A| \ge L_0\,(1 - 10^{-5}) \\) (a jumper's stretch at its tension is ~10⁻⁶; a 4 m jumper with 2 cm of sag still has ~7·10⁻⁵; the same floor bounds the cut from a sag). The jumper's pull holding the clamps back is not modelled, because sections are solved first. So the jumper is reported as *taut* in that case, instead of being drawn at hundreds of kN. This is decided from the length and the clamps' distance alone, before the rod is solved. The clamps' exit angles and the bending stiffness play no part in it. Two situations lead there:
  - **Wind.** Free strain strings swing their clamps apart.
  - **Cold, on a nearly straight jumper.** A jumper with little slack loses it from both sides when it is cold. Its length shrinks with the temperature, and the dead-end clamps move apart as the colder, tighter sections pull their strain strings straighter. For example, a 4.2 m jumper given a 10 cm sag at 15 °C is only \\( \approx 8s^2/3c = 6 \\) mm longer than its chord. At −20 °C it is 3 mm shorter, and its clamps are a few millimetres further apart, so it is taut.

  A jumper that is really a stiff bar (a rigid or tubular jumper) has no slack to lose, so it should not be modelled as a cable with a token sag. Give it `rigid` (below): it is then the straight member between its clamps and is never taut.
- **A section at an end not solved.** The jumper's ends are the solved clamps of the sections through them. When one of those sections fails in a weather case, the jumper fails too and names it, rather than hanging between the attachments' data points.
- **Ends less than 0.1 m apart horizontally.** A vertical jumper is not supported yet.
- **A support string out of reach.** A string's top is farther from either end than the jumper's length to its clamp plus the string's length.
- **Strings that do not settle.** Strings going taut and slack in turn 8 times. Not met in testing.
- **No convergence.** The message names the residual. This is rare since the solve runs in coordinates relative to the first end.
- **Sag cannot be reached.** The sag asked for is less than the taut jumper's, or would need more than 20× the chord length.

## Expected ranges

Physical values for typical high-voltage jumpers (indications to check a model against, not limits):

| Quantity | Typical range | Notes |
|---|---|---|
| Length | 3–15 m | longer on wide dead-end crossarms and taps |
| Sag in still air | 0.5–3 m | clearance to the structure usually decides it |
| Tension | 50–1 000 N | tens of newtons for a short light jumper, hundreds with ice or wind |
| \\( EI \\) (estimated, \\( k = 0.2 \\)) | 15–1 500 N·m² | grows with \\( E d^4 \\): ~17 for a 13 mm wire, ~455 for Drake 26/7 (28 mm), ~1 500 for a 40 mm conductor |
| Reduction factor \\( k \\) | 0.2–0.35 | default 0.2 |
| Clamp exit angle | 0–45° below the conductor's axis | default 30°; 0: straight on |
| Change of sag between −20 °C and 80 °C | a few cm to a few dm | the length changes by \\( \varepsilon \approx 2 \cdot 10^{-3} \\); the clamps move too, so the sag below the chord can even shrink when hot |

Computation (release build; times for one core — jumpers are independent, so a weather case solves them on all the cores):

| Step | Expected cost | Exit clause |
|---|---|---|
| Catenary element solve | ~1 µs | 100 iterations, 20 ms |
| Rod Newton iteration (\\( 2n+3 \\) unknowns, \\( n \approx 24 \\)) | ~50 µs | — |
| Rod solve (5–30 iterations) | 0.25–1.5 ms; ~0.75 ms on average | 150 iterations, 60 s |
| Rod solve with a support string | a few times the above | string steps no smaller than 1/64 of the way |
| Rod solve with support clamps (I or V) | 2–5 ms for a 10.8 m jumper (sections included) | the same per solve; 8 rounds of strings going taut or slack |
| Cut from a sag, hinged | ~50 µs | 200 iterations; 20× chord |
| Cut from a sag, clamped | 5–15 ms (end sections plus 5–15 rod solves) | 40 iterations; 20× chord |
| A weather case, 433 clamped jumpers | ~0.3 s on one core, divided by the number of cores | — |
| An edit of one section (its jumpers only) | ~10 ms | — |

A weather case is solved in full the first time it is shown, which costs the line's jumpers once; after that only edits re-solve them.
