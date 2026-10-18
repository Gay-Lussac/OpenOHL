# Jumpers

Implemented in `crates/ols-line` (`jumper.rs`, `rod.rs`). A jumper is a short slack cable joining two attachment points: the ends of two sections at a dead-end structure, or a tap from a branch to a through wire. The line file fields are described in [Line files](../user-guide/line-file.md#jumpers).

**Its load on the clamps** (Criteria › Calculation options, the line's `solver.jumper_loads`). The sections at a jumper's ends carry it at their clamps — clamps hanging from strings; a wire clamped to the structure leaves it to the structure — in one of two ways:

- **An estimate** (`estimate`, the default): in each weather case, each end carries the **weight, ice and wind** of the part of the jumper between it and its first support — half of it without supports — as a force at the clamp (`jumper::end_loads`). The wind is the line's, as on the jumper itself (step 3 below). The length for that is the cut length, else the length giving its sag between the wire points in still air.
- **The jumper's own pull** (`pull`): the force the solved jumper exerts on each clamp — its tension at the end along it, with its stiffness for a stiff jumper (the moments of clamped ends left out) — replaces the estimate (`JumperShape::end_forces`, `solve::jumper_pull`). The sections are solved with the estimate, then the jumpers between their clamps, then the sections whose clamps' loads changed by more than 0.5 N (with the sections sharing their assemblies), and so on until nothing changes (at most 8 passes; two or three in practice). A clamp whose jumpers did not all solve keeps the estimate. Cost, measured in release builds: on a test line of 31 sections (21 at its 15 jumpers), 4 weather cases, 0.29 s with the estimate and 0.91 s with the pull (about 3×); on an existing 25 km line where nearly every section ends at a jumper (554 sections, 518 jumpers), 99 weather cases, 32 s and 158 s (about 5×; the sections are solved over the cores, the jumpers too, the groups sharing assemblies one after the other). The sections away from jumpers are not solved again, so the factor falls with fewer jumpers.

**Not in the computations:** the **moments** the jumper puts on a clamp — the bending moment of a stiff jumper's clamped end and the **torsion** it twists the clamp and the conductor with — and the conductor's and the clamp's torsional response to them. Only forces reach the sections (with the estimate, or the jumper's pull). The effect is small next to the forces (a clamp's lever arm is a few centimetres) and is deliberately left out.

Either way the jumper is not there at stringing (jumpers are hung after the wires are strung), so a section with jumpers solved at its stringing temperature carries a little more tension than it was strung to. A section is solved again when the jumpers on it, or the choice, change. The ruling-span method leaves jumpers out, as every point load.

With the estimate, **sections are solved first** and each jumper is then solved on its own between the sections' solved clamps (a jumper's pull, tens to a few hundred newtons, is small next to a section's tension). The steps, in every weather case:

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

Where an end is the first or last point of a section, the clamp holds the jumper. It leaves at `angle_from_deg` / `angle_to_deg` (default 30°) from its tension string's line (from the clamp towards the string's top; two strings: the middle of their tops; none: the conductor's axis at the clamp), turned within the conductor's plane towards the weather case's load on it (straight down in still air, tilted with the wind as the conductor swings): with the string's base direction \\( \\hat s \\) and the load per metre \\( \\mathbf q \\), the exit direction is \\( \\hat s\\cos\\alpha + \\hat n\\sin\\alpha \\), \\( \\hat n \\) the unit part of \\( \\mathbf q \\) square to \\( \\hat s \\) (along the string: no turn). The load is the jumper's own cable's in the case (a jumper of the line's cable: the conductor's). The cable's bending stiffness \\( EI \\) carries that direction into the jumper. The jumper can therefore not swing in the wind as if hinged.

The model is an **inextensible elastic rod**:

- **Pieces.** \\( n \\) straight pieces of equal length \\( \ell \approx 0.12 \\) m, with \\( 16 \le n \le 40 \\). The unknowns are the pieces' unit directions \\( d_0 \dots d_{n-1} \\). Node \\( m \\) is at \\( A + \ell \sum_{i<m} d_i \\).
- **Energy.** The load's potential, plus a bending term \\( c\\,(1 - d_i \cdot d_{i+1}) \\) at every inner node and \\( 2c\\,(1 - d \cdot e) \\) at a clamped end (\\( e \\): the direction the clamp holds), with \\( c = EI/\ell \\).
- **Constraints.** The far end is at \\( B \\): \\( \ell \sum d_i = B - A \\). A taut support string keeps its node at the string's length from its top.
- **Solution.** Newton's method on the stationarity of the Lagrangian. Each direction is rotated within its normal plane, so the pieces keep their exact length and short stiff pieces never stretch artificially. Levenberg–Marquardt damping and a backtracking line search keep the steps safe.
  - **Start:** the hinged catenary under the same load.
  - **If that doesn't converge:** \\( EI \\) is brought in in steps (0, 5 %, 20 %, 50 %, 100 %), each step starting from the last.
  - **With a support string:** the string is first left out, then shortened to its length in steps.
  - **If neither converges: through springs** (below, [A jumper locked on its way to its strings](#a-jumper-locked-on-its-way-to-its-strings)).
- **Tolerance.** Moments to \\( 10^{-9} \\) of the load and bending scale; closure gap to \\( 10^{-10} \\) of the length. The solve runs in coordinates relative to \\( A \\): in project coordinates (~10⁶ m) rounding would swamp these tolerances.
- **Exit clauses.** 150 iterations and 60 s per solve (400 through springs). The error says in words what is wrong where it can (a jumper too short for its strings), else states the residual as a multiple of the tolerance.
- **Stretch.** The cable's stretch is left out, because it is ~10⁻⁶ at a jumper's tension.

## Support clamps: a rod with rigid pieces

A jumper can pass through clamps between its ends (`supports`), each hanging on **suspension strings of the structure** — ordinary strings, nothing of their own ("helping strings" is only a name for what they do): one string (an I) or two strings to one wire point (a V). The clamp is free to swing and turn, so no angle is assumed for it. The cable runs straight through it: over the clamp's few centimetres it cannot bend, so the direction it leaves in is the direction it enters in.

**Which wire points a jumper passes through.** A clamp hangs on a **wire point without a set**. A wire point with a set is where a wire ends: a section's end, or a jumper's. So:

- the **Add jumper** tool takes a right-click on a string to a wire point *without* a set as a clamp the jumper passes through, and a right-click on a wire point whose set a section uses as the jumper's other end (it is then made at once). A wire point with a set **no section uses** is refused, with the reason: a string meant to hold the jumper was given a set by mistake — accepted, it would split one jumper into several;
- the **Add string** tool gives a new string's wire point the next free set. To hang a jumper's clamp on such a string, untick its wire point's **set** in the string window first.

The wire points a jumper passes through, in order, are shown in its window (with the lengths between its clamps), in its properties ("passes through JH on T5, then …") and in the Jumpers table (clamps). Laid with the tool — its first end, the strings it passes through in order, its other end — a jumper with clamps is given by the **lengths between its clamps** (its sag depends on the clamps, so it cannot be given by a sag): each first 3 % over the straight distance between the clamps, to be typed in its window; its length is their sum, and each clamp's place along it (`at`, a share of the length) follows from them.

A jumper with support clamps is always solved as the rod above, also with \\( EI = 0 \\) or hinged ends:

- **Pieces.** The pieces now have their own lengths \\( \ell_i \\); node \\( m \\) is at \\( A + \sum_{i<m} \ell_i d_i \\). Each clamp is **one piece** of its length (`clamp_length`, default 0.1 m). One straight piece has one direction, so the jumper is straight through the clamp by construction, whatever its stiffness. The stretches between the clamps and the ends are cut into pieces of equal length, 16 to 40 in all (~0.12 m), at least one each, in proportion to their lengths.
- **Bending.** \\( c_i = EI / \bar\ell \\) at the node between pieces \\( i \\) and \\( i+1 \\), with \\( \bar\ell \\) the mean of their lengths; \\( 2EI/\ell \\) at a clamped end, with that end piece's length. With equal pieces this is the rod above.
- **Where the strings hold.** At the clamp's middle: \\( p = x_k + \tfrac12 \ell_k d_k \\) for the clamp's piece \\( k \\). The strings therefore turn the clamp too: it settles where the cable's pulls at its two ends and the strings balance.
- **Strings.** Each string is a rigid, tension-only distance constraint from its fixed top (a structure point) to the clamp's middle: an I is a pendulum, a V two such constraints. A string is **slack** while the clamp is nearer its top than its length, or while its force would push. The solve keeps an **active set**: all strings are first left out; a string whose point then hangs beyond its length goes taut and is shortened to its length in steps (as a single support string is); a taut string whose force would push is let go; this repeats until no string changes. A V leg on the lee side of a strong wind goes slack this way. A string with a `compression` (it may push) is taut from the start and never let go: it holds the clamp at its length whichever way, its force negative when it pushes.
- **Loads on a clamp.** Half of each string's weight, half of the wind on each string (the weather case's pressure × the string's `wind_area`, horizontal across the line from the case's wind side, as on the line's suspension strings), and the wire point's own `weight` (a counterweight), shared by the subconductors. The clamp itself takes no wind (it is small).
- **Forces.** A string's tension is \\( T = -2 \mu L \\) from its multiplier \\( \mu \\) (constraint \\( |p - t|^2 = L^2 \\)); 0 when slack. The Jacobian of the system, with the strings' rows and columns, is checked against finite differences in the tests.
- **A string that may push** holds its point on a sphere about its top; a V of two such legs, on a circle about the line through their tops. When the tops are one above the other the V swings like a gate about them, and the circle lies across: which way it swings depends on where the jumper is pulled from (see below).
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

## Objects on a jumper

A jumper's labelled points ([points and objects](../development/line-points-and-objects.md))
are placed by a share of its length from `from`, a length along it, or a
horizontal distance from `from`'s clamp. An object on one (a body, a
clamp) loads the jumper there: its weight and ice down, the wind on it
across the line from the case's side (`LineObject::forces`), each
subconductor its share. A jumper with objects is solved with the rod
(as with support clamps): each object is a load held nowhere, on the
piece covering its place; its stiffness, ends and support clamps as
otherwise. Its points are reported where they are solved
(`JumperShape::point_positions`). The sections at its ends carry it with
either option: the pull (its end forces include the objects), or the
estimate (an object between an end and the first support shared between
them by where it is; without supports between the two ends; between
supports, none). A rigid jumper shares each object between its ends by
where it is. A jumper given by its sag is cut without its objects
(fitted afterwards, as spacers on spans).

## Jumpers on taps

A jumper's end may be a **tap** instead of a section's end: a clamp
object on a span point (a T-off from a conductor in the span) or on
another jumper's point. The clamp is a hinge: it passes forces only, so
the jumper leaves it in the direction its own equilibrium gives (no exit
angle; the rod's end is left free) and the wire takes no moment nor
torsion (the wires carry none anyway). The end is where the point is
solved in the case. A tap jumper is given by its length (or rigid): a
sag would need a design case in which the tap stays put.

What the wire it hangs on carries (Criteria › Calculation options,
*Jumpers on taps*):

- **The full loop** (default): the jumper's own force at its end put on
  the wire at the tap point (a force on that node of the section's FE
  model, or a load at that point of the host jumper's rod), the wire and
  the jumper solved again in passes until the tap no longer moves (less
  than 0.5 N change, at most 8 passes — the same loop as the jumpers'
  pull on clamps, which stays the separate *Jumpers on the clamps*
  choice). Cost: the sections with taps and their jumpers solved again,
  two or three passes; the others untouched.
- **One calculation**: the wire carries the jumper's estimate (its
  weight, ice and wind up to its first support, half without) at the
  tap, then the jumper is solved once.

Jumpers on jumpers are solved after the jumper they hang on, level by
level (each level over the cores); a ring is refused. A tap joins the
jumper to its wire's conductor path (electrical circuits) and connects
it to that wire for the checks (not paired with it in the distance
between conductors, as two wires sharing a clamp). A span with a tap is
solved by finite elements only (not the ruling span).

## Rigid jumpers

A jumper given `rigid` is a tube or bar: the straight member between its two solved clamps, in every weather case. It has no length or sag of its own and no thermal change. Its tension is not computed, because the members holding it carry it. It is drawn, and checked for clearances, along that straight line, and it can never be taut.

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

## Taut jumpers

- **Taut.** The clamps end up at least as far apart as the jumper's unstressed length in that weather case: \\( |B - A| \ge L_0\,(1 - 10^{-5}) \\) (a jumper's stretch at its tension is ~10⁻⁶; a 4 m jumper with 2 cm of sag still has ~7·10⁻⁵; the same floor bounds the cut from a sag). This is decided from the length and the clamps' distance alone, before the rod is solved. The jumper is then solved as an elastic catenary between its clamps (its bending stiffness, small beside its tension, left out): stretched, at a tension \\( T \approx EA\,(|B-A| - L_0)/L_0 \\). Its pull holds the clamps back: the sections at its ends are solved again with the jumper's end forces on their clamps, whatever the jumper option (the estimate means nothing for a taut jumper), the step relaxed with Aitken's method (the coupling is stiff: a small move of a clamp changes the pull much), up to 40 passes, until the loads change less than 0.5 N. Its largest tension is compared with the cable's rated tensile strength (`JumperShape::strength_use`). With helping strings or points along it, a taut jumper is not solved (said so). Two situations lead there:
  - **Wind.** Free strain strings swing their clamps apart.
  - **Cold, on a nearly straight jumper.** A jumper with little slack loses it from both sides when it is cold. Its length shrinks with the temperature, and the dead-end clamps move apart as the colder, tighter sections pull their strain strings straighter. For example, a 4.2 m jumper given a 10 cm sag at 15 °C is only \\( \approx 8s^2/3c = 6 \\) mm longer than its chord. At −20 °C it is 3 mm shorter, and its clamps are a few millimetres further apart, so it is taut.

  A jumper that is really a stiff bar (a rigid or tubular jumper) has no slack to lose, so it should not be modelled as a cable with a token sag. Give it `rigid` (below): it is then the straight member between its clamps and is never taut.

## A jumper locked on its way to its strings

The strings are pulled to their lengths in steps from where the jumper hangs without them, all together, each step an equilibrium. That is a path: winching the strings in from a jumper blown away from them. On it a stretch between two held points can come straight before every string reaches its length — the solve then stops with "no step lowers the residual" or does not converge, its forces growing without bound — although a shape holding every string exists, reached from elsewhere. On the first existing line, a jumper with two I strings and, near its far end, a V that may push with its tops one above the other (a gate) did so in the wind from one side: pulled up by the second I string, the V swung the wrong way and the stretch to it came straight; the shape holding everything has the V swung far the other way. Its energy minimum is the same from every start (checked apart, with a general constrained optimiser), so the solution is unique; only the path was wrong. The tests rebuild it with an invented jumper of the same kind.

Where the steps and the steps in \\( EI \\) both fail, the jumper is solved **through springs** (`Rod::solve_through_springs`):

1. its strings become springs of stiffness \\( k \\) (energy \\( k g^2 / 8L^2 \\) for \\( g = |p - t|^2 - L^2 \\), about \\( k\delta^2/2 \\) for a stretch \\( \delta \\); a tension-only string only while stretched), from soft to stiff: \\( k = 10^{-2} \dots 10^{3} \\) times the loads per metre (the rod's load times its length plus the loads at points), each solve starting from the last, each only roughly (to \\( 10^4 \\) times the tolerance: a start). A soft spring lets the jumper find where every string can hold it, swinging the V whichever way lowers the energy; the Jacobian with the springs is checked against finite differences in the tests;
2. then the strings rigid again from there: those stretched are taut at once (they are within millimetres of their lengths), and the active set as above;
3. **too short:** if that fails too, the springs stiffer still (\\( 10^4, 10^5, 10^6 \\)); a held point still more than 1 mm from where its strings hold it (the least move putting its stretched strings back on their lengths, to first order) says that the jumper cannot reach: *"too short to reach where its strings hold it: they hold the point 2.50 m along it about 10 mm beyond its reach (strings that may push)"* (a test's). On the first existing line, the wind of an extreme case swung a jumper's end clamps apart by a few millimetres more than its two Vs that may push let it reach.

Newton solves on this path that stall or run out of steps within \\( 10^3 \\) times the tolerance (\\( 10^{-6} \\) of the loads' moments: micrometres) are taken as converged: next to a string just taut, carrying next to nothing, the iteration creeps there (100–150× on the first existing line). The first two attempts (the steps, the steps in \\( EI \\)) are unchanged, so a jumper that solved before keeps its shape.

On the first existing line (99 weather cases, the wind from both sides): before, one jumper with a V that may push failed in most cases with the wind from one side, another (an I string) in a few cases; now all solve but one jumper in one extreme case, a few millimetres too short for its two Vs that may push, said so. Every jumper that solved before has the same shape.

## When a jumper fails

- **A section at an end not solved.** The jumper's ends are the solved clamps of the sections through them. When one of those sections fails in a weather case, the jumper fails too and names it, rather than hanging between the attachments' data points.
- **Ends less than 0.1 m apart horizontally.** A vertical jumper is not supported yet.
- **A support string out of reach.** A string's top is farther from either end than the jumper's length to its clamp plus the string's length.
- **Too short for its strings.** No shape holds every string, although each alone could reach (through springs, above): said so, with about how far beyond the jumper's reach its strings hold which point.
- **Strings that do not settle.** Strings going taut and slack in turn 8 times. Not met in testing.
- **No convergence.** Not by the steps, nor the steps in \\( EI \\), nor through springs: the message says so and names each one's residual. This is rare since the solve runs in coordinates relative to the first end.
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
| Clamp exit angle | 0–45° from the tension string's line, towards the load | default 30°; 0: along the string |
| Change of sag between −20 °C and 80 °C | a few cm to a few dm | the length changes by \\( \varepsilon \approx 2 \cdot 10^{-3} \\); the clamps move too, so the sag below the chord can even shrink when hot |

Computation (release build; times for one core — jumpers are independent, so a weather case solves them on all the cores):

| Step | Expected cost | Exit clause |
|---|---|---|
| Catenary element solve | ~1 µs | 100 iterations, 20 ms |
| Rod Newton iteration (\\( 2n+3 \\) unknowns, \\( n \approx 24 \\)) | ~50 µs | — |
| Rod solve (5–30 iterations) | 0.25–1.5 ms; ~0.75 ms on average | 150 iterations, 60 s |
| Rod solve with a support string | a few times the above | string steps no smaller than 1/64 of the way |
| Rod solve with support clamps (I or V) | 2–5 ms for a 10.8 m jumper (sections included) | the same per solve; 8 rounds of strings going taut or slack |
| Through springs (only where the above fail) | 7–10 rough Newton solves more: 20–150 ms for a 17 m jumper | 400 iterations, 60 s per solve |
| Cut from a sag, hinged | ~50 µs | 200 iterations; 20× chord |
| Cut from a sag, clamped | 5–15 ms (end sections plus 5–15 rod solves) | 40 iterations; 20× chord |
| A weather case, 433 clamped jumpers | ~0.3 s on one core, divided by the number of cores | — |
| An edit of one section (its jumpers only) | ~10 ms | — |

A weather case is solved in full the first time it is shown, which costs the line's jumpers once; after that only edits re-solve them.
