# Lay out joints

![The Lay out joints window: the joints, the settings, the preview with what changes, the joints left for a hand detail and a joint's close-up before and after](../images/structure-tools/tool-lay-out-joints.png)

(2026-11-18; built, in active development.) **What it is for:** places
the member ends of a tower's joints so that they physically fit — no
steel through steel — once the members are chosen: each end pulled back
(where its steel stops), moved sideways only where that is not enough,
and bolted onto the leg directly or onto a gusset plate. The bolts
themselves are not chosen here (that comes with the design codes).

**Setting it up**, in order:

1. Choose the members' cross-sections, rolls and classes (`bracing`,
   `redundant`…) first: the layout works from them. A tower without
   orientation data (an import, a tower drawn by hand): **Orient angles**
   first ([below](#orient-angles)), so that its legs stand heel out with
   their flanges in the tower's faces.
2. Open **Design › Lay out joints…** (the joints picked, else the whole
   tower), or **Lay out joint…** in Joint details (that joint).
3. Choose **which joints**: the joint shown, the joints picked, a member
   group's joints or the whole tower; tick **With their similar and
   symmetric joints** to lay those out alike (a run plans one joint of
   each kind and applies that plan to the joints like it, checked in each
   one's own place — [J4](#apply-this-joints-detail-to-similar-joints)).
4. Check the **settings** (each one's effect shows at once in the
   preview below): the gap between steel, the longest pull-back, the shift
   limit, the bolt for ends without bolts (and a connection preset per
   member class), the plates of a corner bolted to their flanges or welded
   together, and whether values given by hand are replaced.
   Below the preview, **Detailing practice** (2026-11-25, L24) says which
   shop practice the layout follows and where it comes from — this
   structure file's own `[layout_practice]` over the code plugin of its
   member check, value by value, over the program's default — and, under
   *Its values*, each preference with where it comes from (this file, the
   code plugin, the default) and its effect: the fabrication order (the
   forms bolted directly, one flat plate, a plate with one bend, several
   bends, a plate welded in the member's own plane, each allowed or not;
   the order decides only ties), the faces tried first, the flatness, the
   bend, the plates' shapes, the picked bolt's pattern (per member class
   too). A practice given with a value out of range is not used (red,
   with why; the default is used, and the run's notes say so). *Take its
   gap, pull-back and shift* puts the practice's starting values into the
   settings above; the settings stay your choice for the run, and where
   they go beyond the practice given (a smaller gap, a longer pull-back,
   a larger shift) a line under them says so, in the warning colour, and
   so do the run's notes (2026-11-26, L29). **Edit…** opens
   [Detailing practice](detailing-practice.md), the editor of this
   structure file's own practice — the plate thicknesses among its values
   (L29). A structure's own practice is written in its file as
   `[layout_practice]`
   ([theory](../theory/joint-layout.md#the-detailing-practice-l24),
   [the table](../development/plugins.md#detailing-practice)).

5. Read the **preview**: how many joints are laid out, how many plates,
   how many member ends of how many are laid out,
   the joints that need a hand detail — which members, the rest laid out,
   those whose place is kept free — and why, the clashes at these joints
   before and after, the time it took (measured); **What changes** per
   joint; a joint's close-up **before** and **after** (drag to turn both);
   **Usage before and after** (the members' largest usage over the last
   run's cases, as the file is and as laid out — the analysis runs twice).

   ![A joint before (D1 and H1 running into each other, red) and after (pulled back onto plate G1, no clash)](../images/structure-tools/tool-lay-out-joints-before-after.png)
6. **Pieces in line** (2026-11-19, J9): two pieces of one section in
   line through a joint with nothing given there of how they are joined
   (a horizontal in two members at a mid-panel joint) are **proposed one
   part** — *Confirm* per joint (under *Each joint*) or **Confirm all**,
   one undo step each; the preview is then worked out again. Until you
   confirm, each piece **ends at the joint** in the joint checks and in
   the layout (on the safe side): what bolts onto it keeps its bolts on
   that piece's steel. Pieces of a leg with bolts are leg splice places
   (next point); a splice, a kind, what it bolts onto or bolts given at
   the joint: not proposed, said why.

   ![Lay out joints on an invented tower: a horizontal in two pieces H1 and H2 through joint M with a redundant on it, proposed one part (Confirm, Confirm all)](../images/structure-tools/tool-lay-out-joints-in-line.png)
   **Leg splices** (2026-11-19, J7): where a leg's pieces meet in line,
   the preview proposes the splice places by the owner's rule — **a
   change of section is a splice** (covers outer and inner on each leg
   bolted, as thick as the thicker piece's leg, the bolts the import
   gives, else provisional ones: *Assumed: …* under each joint says what);
   **the same section is one part** unless you mark a splice there (give
   it in Joint details › + Splice: it is then not proposed one part).
   *Confirm* per joint (under *Each leg joint*) or **Confirm all**, one
   undo step each; never written silently — until you confirm, each piece
   ends at the joint (on the safe side). Once a splice is confirmed, the
   bracing at that joint bolting on a leg's outer face is laid out
   **through the splice's outer cover** (seated on it, its bolts through
   the cover, the leg and the inner cover; the cover checked for the
   bracing's whole force) where its bolts land on the leg's flange clear
   of the splice's own bolts; else it needs a hand detail (a plate over the
   cover is not laid out). A piece of one section confirmed one part has
   its bolts **not checked there — NOT ON THE SAFE SIDE where a splice of
   one section is really there** (a failure of those bolts would not be
   found): mark such splices first.

   ![Lay out joints on an invented tower: a leg in two pieces of other sections C1 and C2 through joint J with a brace D, the splice proposed (Confirm, Confirm all) with what is assumed](../images/structure-tools/tool-lay-out-joints-leg-splices.png)
7. **Apply**: one undo step. Escape or Cancel: nothing changes.

**How the layout places a member end** ([theory](../theory/joint-layout.md)):

| Step | What happens |
|---|---|
| Members in line first | the biggest pair of angles in line through the joint (its base): **one part** when they are the same section, turned alike and have no bolts there (a splice with bolts stays a bolted splice: its bolts stay checked); the smaller **nested** in the bigger when it fits inside; other sections: not joined by the run — since 2026-11-19 (J7) a splice is **proposed** (*Leg splices*) and written only when you confirm it. One section in line with bolts: where the splice is is not known (their bolts may be the members' design data, at both ends) — left as given, each ending at the joint, said; confirm the leg splice places (*Leg splices*) or give the splice |
| Who arrives | each member end bolting by one leg of an angle onto the flange of a member at the joint (the leg, a chord, a diagonal a redundant meets); members in line with it (a splice, one part) are left alone; an end bolting onto nothing gets the member running through the joint (of two pieces in line, the one on its side); **two ends that would bolt onto each other** (the pieces of a horizontal onto the redundant standing on them, the redundant onto them): the one running through, else the larger, stays what the other bolts onto, its own end left as it is, said — never both cut back onto each other; an end whose axis lies in neither flange's plane of the joint's leg (a plan diagonal at a vertical post) gets the member running through in whose flange's plane it lies (the horizontal), if there is one — named first, so that its leg lying on it is the one bolted. **Searched** (2026-11-20, L1): an end left for a hand detail is tried on the other members it could bolt onto at the joint (single angles, its axis in a flange's plane; those running through first, in the same order), at most two each; a joint takes another member only where that lays out more of its ends, also after the clash check (else it is laid out as before) |
| Laid flat | a member lying more than 2° off what it bolts onto (the owner's value of 2026-11-19; 5° before) is turned (its roll) **only where that helps**, **one roll for the whole member** (J3, 2026-11-19): where all its ends agree within 2° (the middle of their turns: an end that was flat stays flat, at joints not laid out too) — any turn, the smallest that does it; an **equal-leg** angle may bolt its other leg at one end (a quarter turn more), as a member running from a horizontal flange to an upright one does —; **class by class**: the legs and chords are not turned to suit what bolts onto them, then the diagonals, the secondary bracing (horizontals, plan and hip members, and members without a class — said), the redundants last, each class turned with the ones before already in place; no end of it at a joint not laid out is laid out with its roll now, and at least one of its joints is laid out with the turn; otherwise its roll stays and the preview's notes say why. A roll the layout set is taken back when none of its joints is laid out with it any more. **Half a turn** where an end on a flange's inner face cannot be laid out there (it runs towards the angle's other leg): its outstanding leg to the other side, kept only when more of its ends are laid out and no joint of it loses one (an X-brace: one diagonal on the outer faces of the legs, the other on the inner faces, back to back where they cross). A roll given by hand is kept unless replaced; a leg's or chord's roll set by **Orient angles** is always kept. **Bottom up, then out** (2026-11-20, L1): a member that no one roll lays flat at all its ends, none of them flat now, is turned to lie flat at its end on the tower's **body** first (the lower first), its ends on the arms and the top taking what comes; the body, the arms and the top are found from the legs' lines (no names). Joints alike (J4) are planned at the first of them in that order |
| Concentric first | its gauge line through the work point (drawn so; the analysis keeps the member's axis there, the code's slenderness curve takes the eccentricity gauge to centroid); its end pulled back until its steel clears the flange's heel (on the inner face: the angle's other leg and root), the ends placed before it and the joint's fixed steel (what is not laid out: members in line, built up, given by hand, left for a hand detail; their bolts) by the gap |
| Searched (2026-11-19) | each joint is **searched**, not filled one member after the other: the planes on the biggest members first (what bolts onto a member laid out keeps clear of it as laid out); in each plane every order that can matter, the owner's order first (the diagonals, then the secondary bracing, then the redundants — owner 2026-11-19 —; then the wider leg, the larger angle), backtracking where an end does not fit — the most ends laid out wins, then the least moved sideways, then the owner's order. Typically a short redundant placed first and the bracing pulled back past it. At most 400 tries per joint (said where reached). **Clash-aware** (2026-11-20, L2): while planning, the layout sees what the clash check after writing sees — each member's outstanding leg to its full height, the nuts behind the flange or the plate, each plate's outline as it will be drawn (round the work point, the members' legs over their bolts, the flange under it) — and a member bolting onto a member laid out at the same joint is planned where that member will be drawn. A plate that would run into steel already there **stops short of the work point** (since 2026-11-20): round its members' legs from the flange's toe over their bolts (each bolt's end and edge distances kept) and the flange under them, cut straight short of what it would still run into (at most four cuts; it must still lie on the flange over half a square of its width) — else it is not laid out (its members bolted directly where their bolts land, else left, said: "… the plate would run into …"). The plate checks take the outline as cut |
| A shift | only where that needs more than the longest pull-back (or more than the member's half less its bolts): the smallest sideways move that fits, searched every 5 mm (since 2026-11-27, O44 L32; before, a millimetre at a time — at most 4 mm more than the least, each place checked), up to the shift limit. The analysis takes it as an eccentricity (a rigid end offset) |
| Onto the leg or a plate | directly when every bolt of the plane lands on the flange (clear of its other leg and root by the nut, the edge distance from its toe — the code's when its plugin gives one, else the structure's default minimum, assumed, not a code's) and, where the member it bolts onto ends at the joint (pieces in line not confirmed one part, a member whose own end is laid out there), on that member's steel past its cut by the end distance; else every member of the plane onto one plate on that flange, its bolts past the flange by the nut and the gap — on the outer or the inner face. Plates: as thick as the thickest leg bolted on it, outline round the bolts (the code's end and edge distances) cut at the flange's heel by half the gap (on an inner face: at the angle's thickness, root and half the gap from it, clear of its other leg); at a corner one per face, bolted to its flange, or welded together. Each plate the layout makes names the member whose flange it lies on (`on`, since 2026-11-20: it is drawn, seated and tested for clashes on that member — before, a plate on a bracing member's flange was taken to lie on the joint's leg; the file is then written at version 23). **Slightly more plates** (2026-11-20, L1): where the joint would leave members for a hand detail, a face bolted directly is also tried on a plate (taken only where that lays out more — bolting directly wins a tie), and an end bolted directly that would be left for a clash is tried on a plate first. An end on a plate lies the plate's thickness farther out of the flange: not in the analysis, as for every plate (**NOT ON THE SAFE SIDE** by that offset: its moment is missing, so a usage can be too low; the estimated eccentricities take it) |
| Off the flange | a member that no roll lays flat at both its ends (2026-11-19): up to 20° off (its ends needing rolls a few degrees apart; since 2026-11-20, L3) first onto a **plate bent by as much** from the flat plate of its face on the flange — one plate, no weld, its bend clear of the member's bolts by their edge distance, turned away from the flange or (past the flange's toe) towards it, whichever moves the member less; the end that lies flat is the one flat now, else the one first bottom up, then out (so an arm's or the top's end takes the bend). Else, or more than 20° off, onto a **plate in its own plane**, welded along the line where it meets the **flat plate** of its face on the flange (the other ends of that face bolted onto the flat plate) — both plates as thick as the thickest leg on them, the weld from both sides (each side a throat of 0.4 × that thickness in whole millimetres, at least 3 mm: the layout's choice, written so the weld is checked), the member's bolts past the flange's toe. On the flange's outer face, or its inner face for a member coming from inside the leg's corner. Only where the member then lies at most one plate thickness farther out of the flange than on a flat plate (its line of action off the work point is not in the analysis); else, and where the flat plate would not reach the flange or would pass too near another end's bolts, a hand detail, said |
| No fit | the member is left exactly as it is and listed with why ("R1 and D1 within 3°: no room on one plate …"); the rest of the joint is laid out. A member left for its own sake (no roll lays it flat, too short, built up…) **keeps its place**: the ends laid out keep clear of it as it is, so the joint needs only that member by hand. Laid out, a member whose steel, bolts or plate would run into anything is first pulled back past it (up to three times), then left as it was with the clash; an end laid directly onto a member whose bolts would not fit that member as written (its flange, its steel where it ends at the joint — the joint checks' rule; e.g. that member left as it was) is left as it was too, said ("laid out, … would run 6 mm into each other"); likewise (2026-11-20) an end whose holes in the member it bolts onto would overlap another end's (one on the flange's outer face, one on its inner face), and the members on a plate whose own bolts onto the member it lies on would lie outside its outline or beyond that member's steel — and the rest of the joint laid out again without it (up to six rounds; then the joint is left whole, as it was). Never forced |

**The window**:

| Field | What it does |
|---|---|
| Joints | the joint shown · the joints picked · a member group's joints · the whole tower; with their similar and symmetric joints |
| Gap between steel (mm) | between the steel of members that do not bolt to each other, and from their bolts' nuts. **Not from a standard** (the owner gave none): 10 mm to start with |
| Longest pull-back (mm) | how far a member's steel may stop from the work point before a shift is tried. Not from a standard: 300 mm to start with (measured on the 78 tower files of a real line: 95 % of the ends laid out need at most 136 mm on every tower, the largest 344 mm) |
| Shift limit (mm) | the largest sideways move of a gauge line off the work point (0: never moved). Not from a standard: 30 mm to start with (measured on the same towers: 37 of 18 289 ends needed a move, the largest 20 mm) |
| Bolt where an end has none | two bolts of it in one gauge line, **provisional** |
| Preset for bracing, redundant, crossing, leg | a connection preset's bolts for an end of that class without bolts (before the bolt picked); provisional too |
| Plates at a corner | each bolted to its flange (usual lattice practice) · welded together |
| Replace values given by hand | off: a cut, shift, roll or what a member bolts onto given by hand is kept (the layout works round it); on: replaced |
| Solve the clashes at the joints left after the run | off by default: after the run, [Solve the clashes](#solve-the-clashes-at-the-joints-left-after-the-run) at every joint it leaves not whole, kept only where better; every turn and move said (marked). Slow: about 20 s per joint left, more on a busy machine — hours on a large tower (measured) |
| Pieces in line: **Confirm all (n)**, *Each joint* › **Confirm** | writes the pieces proposed one part (`kind = "continuous"` at both ends; a little apart in roll: one roll half-way, the pieces already one part with them included), one undo step; the preview is worked out again. Those not proposed are listed with why |
| **Apply (n joints)** | writes the preview, one undo step; greyed while the preview is worked out |
| Cancel, Escape | closes the window, nothing changes |

**Good to know:**

- **Laid out once per kind** (2026-11-19, J4): joints alike by the tower's symmetry or a repeated panel (members, sections, rolls, classes and what the layout reads of them alike) are planned once; the others take that plan, mirrored or turned, and are checked for clashes in their own place — where it does not fit there, the joint is planned on its own. The preview's line of numbers says on hover how many joints took the plan of a joint like them.
- What the layout set is **automatic** (the file's [record](structure-library.md#the-joint-layouts-record)): Joint details says *laid out automatically* (on hover: which values), *provisional bolts* and *Needs a hand detail: …*. A value you change becomes yours and is kept.
- **It follows later changes** (in about a tenth of a second on a tower of 3 000 members: only the joints touched, worked out from what they read): an edit near a laid-out joint — a plate made thicker, a member's section, roll or bolts, a value at an end, a joint moved, a setting — lays that joint out again in the same undo step, your values kept, the bolts' grips following the plates. If it no longer fits, the edit is kept, the joint's layout stays as it was and the edit's message says *no longer fits, its layout kept, needs a hand detail* with why.
- **Provisional bolts keep the tower at level 3** for the joint details — everywhere: OHL-Structure, OHL-Design (the structure picker and the structure's properties say why), `ohl structures fe` and the line's calculation level 4. With `joint_details = true` the calculation says so and checks the members in the simple form until no bolt is provisional.
- Where the design code gives **no minimum bolt distances** (the simple check, a plugin without them), the layout keeps the structure's **default minimums** (Design › Analysis, *default bolt minimums*: end and edge 1.5, spacing 3 bolt diameters to start with) — a default minimum, assumed, not a code's. The provisional bolts' pitch and end distance are the default minimums too.
- A member on the **inner face** of a leg (as its roll turns it) is laid out there, directly or onto a plate on that face, clear of the leg's other flange; where it runs towards that flange it is turned half a turn where that helps. A plate on a **built-up** member (two angles back to back or in a star, four in a cross; since 2026-11-20, J6, in active development) lies on the angle of it facing the members, as on a single angle, kept clear of the member's other angles, and only where the joint lays out more ends with it; **between the angles** (since 2026-11-22, L15, in active development) for members whose leg lies across the gap between two of its angles (a plan member flat on a built-up horizontal's backs, say): a plate in the gap as thick as the gap (`between`, structure file version 25), the members on its far face pulled back clear of the other angle, its own provisional bolts through both angles (where the gap is at least half the thickest leg on it; else by hand, said); a plate you add at a built-up leg without naming the member it lies on stays drawn through the work point as before. A member is laid out on the face of the member it goes onto that Joint details then draw it on — also one that names nothing it bolts onto (since 2026-11-22, L16, in active development; before, such a member could be planned on the outer face and drawn on the inner). Members on both sides of one plate: by hand only — except a plate between a built-up member's angles: members across its gap from either side (or on the two pieces of a member one part through the joint) share the one plate, each on its own face (since 2026-11-23, L17, in active development; before, each had a plate of its own in the one gap and the two ran into each other).

  ![Lay out joints on an invented K-brace whose leg is four angles in a cross: the two diagonals laid out onto a plate on the angle facing them, nothing running into the leg's other angles](../images/structure-tools/tool-lay-out-joints-built-up.png)

  ![Lay out joints on an invented built-up diagonal (two angles back to back, a 9 mm gap): before, a redundant across the gap running into the diagonal; after, pulled back onto plate G1 between the angles, 9 mm thick, no clash](../images/structure-tools/tool-lay-out-joints-between.png)
- A **plan or hip member lying level** between two horizontals that end at the joint — their level legs in one plane with it (within 2°, their faces within 1 mm as drawn) — goes onto a **level plate** lying on both level legs, held by its own provisional bolts onto both horizontals (since 2026-11-24, L20, in active development; `[plate.also_bolts]`, structure file version 26): before, such a member lying on none of the flanges of the leg or of the member running through was left for a hand detail. Where the second horizontal's bolts find no room, or its level leg ends up out of the plate's plane once the horizontals are laid out, the plate is held by the first alone and the notes say why. The joint checks take both horizontals' bolts as the plate's holding bolts, each carrying the plan member's force whole (on the safe side) and, since L21, also the force the plate may pass from the other horizontal — each share from none to the whole, the larger bolt by bolt; each horizontal's own end bolts likewise ([theory](../theory/joint-groups.md#the-force-a-level-plate-passes-between-its-two-holders-l21)).

  ![Lay out joints on an invented corner: a plan diagonal between two horizontals ending at a post, laid onto a level plate held by its own bolts onto both horizontals](../images/structure-tools/tool-lay-out-joints-level.png)
- A member that no roll lays flat goes onto a **plate in its own plane** welded to a flat plate on the flange; where that plate puts the member's axis more than a plate's thickness farther out of the flange, it is laid out up to **40 mm** (since 2026-11-24, L20, in active development; the program's value, not a standard's; before: a hand detail): the offset is recorded with the plate and taken by the **estimated eccentricities** (the Results tab's tick, an option at level 3, off by default). Without them — or at level 4 — the analysis says NOT ON THE SAFE SIDE for those plates: turn the estimated eccentricities on to have the offset checked.
- A **flat plate's own bolts** (since 2026-11-20, L5o, in active development): where a plate lies on a member that runs on through the joint and none of the joint's legs bolts onto it, the layout gives it at least two **provisional** bolts of its own onto that member (`[plate.leg_bolts]`; more where one member on the plate has more bolts — the program's starting value, not a code's), within the member's flange, inside the plate and clear of the other bolts there, as the bent plate's. The joint checks then check how the plate is held (its holding bolts and the member's bearing under them) — before, *not checked*. Where they do not fit, on a plate stopping short of the work point whose outline round them would run into something (since 2026-11-21, L9; before: on every such plate), or on a built-up member, the plate is laid out without them and the window's notes say why (its holding stays *not checked*). The run plans a joint without them where it lays out fewer ends with them. At a joint laid out like another (mirrored or turned), the bolts name the leg the map carries the other joint's onto (since 2026-11-20, L7; before, a mirrored joint's bolts could land on the other leg, off the plate, and the run took them back). Since 2026-11-24 (L22, in active development) they are also placed clear of what the layout puts at the joint after that plate — another plate on the same face, the other members and their bolts' heads and nuts (on the member's other flange in the angle's corner too), the other plates' own bolts —; where no place inside the plate is clear they are placed as before and the notes say so (the clash check after writing then tests them, as since L21).
- **A member the layout chose a host for but does not lay out** (since 2026-11-21, L8, in active development): to plan a joint, the layout picks what a member bolts onto where nothing says (the member running through the joint) — but writes it only for the members it lays out. A member it leaves is drawn without it, often elsewhere. The joint is then planned once more with that member where the file draws it, so that the plates and ends around it keep clear of it where it really is; that plan is kept where it lays out more ends, or as many on no more plates, and leaves no member unsaid that needed a hand detail. The window's notes name each such joint. A member with its face given by hand keeps the host chosen. It makes the run slower (measured on six real towers: about a third more, the median 30 → 41 s per tower).
- **The bolts of a member left as it is** (since 2026-11-21, L8, in active development): of two members bolting onto each other, the layout keeps one as it is and lays the other out onto it. That other member now keeps the first one's bolts on its flange: its cut and move only where each of those holes stays within its flange, past its cut by the end distance, and its own bolts the minimum spacing from them (the code's distances, else the structure's default minimums — assumed). Where no cut and move within the longest pull-back and the shift limit does, it is left for a hand detail and the joint says why (before, it was laid out and then taken back).
- **A plate's outline read in its own plane** (since 2026-11-21, L9, in active development): a plate whose outline is in the file (one the layout wrote round its bolts, or one you drew) lies in the plane of all its corners. Before, its plane was taken from its first three corners — in a line where the outline runs straight there, as one written to a tenth of a millimetre often does — and the members on it could be drawn on their other leg, their bolts along the plate. Files laid out before are read so too.
- **An end on a plate on a member drawn off its line** (since 2026-11-21, L9): the plate and the ends on it lie on that member where it is drawn (seated on its own leg, shifted), as an end lapped on it directly has since L7 — also where nothing else bolts onto that member.
- **A welded plate's flat plate** (since 2026-11-21, L9, in active development): the flat plate a plate in a member's own plane is welded (or bent) to is planned with its outline as the layout writes it — round the work point, the seams and the flange under them — and kept clear of what is there (a member lapped on the same face, say). Where it would run into something it stops short of the work point and is cut straight short of it; where that does not fit either, the member in its own plane needs a hand detail and the joint says why. Such a flat plate's clashes after writing now take back the members on its welded plates (before, with nothing bolted on it, they were written unsaid).
- **A plate stopping short of the work point gets its own bolts** (since 2026-11-21, L9, in active development): as a flat plate round the work point (L5o), where its outline grown round them still runs into nothing and keeps each holding bolt its distances from the edge; else none, and the window's notes say why (its holding stays *not checked*).
- **Faster** (since 2026-11-21, L9): the same layouts in about three quarters of the time (measured on six real towers: the median 45 → 33 s per tower with L9's other changes in).
- **Members on either side of a leg's flange** (since 2026-11-21, L10, in active development): where two or more members lie on one face of a leg's flange and go onto a plate there (or one is left for want of room beside another), the layout tries some of them on the flange's other face — half a turn of the member, its outstanding leg to the other side —, the diagonals onto the inner face first, as on many towers (the horizontals on the outer face, the diagonals on the inner one, meeting at one point without a plate). It keeps the turn only where the member's joints then lay out more ends, or as many on fewer plates, and none fewer; and only where the member's leg then lies no farther from the work point across the flange than before (onto the inner face it lies nearer). A turn the other way (from the inner face onto the outer one, farther out) is only proposed in the window's notes, with the bending it would add; turn the member by hand to take it. The move across the flange is not a `shift`: it is where the member sits on the flange, which the calculation leaves out for every member, as before (see the theory page).

  ![Lay out joints on an invented joint: a horizontal and two diagonals meeting a wide leg from one side; before, all three on a plate on the flange's outer face; after, the two diagonals on the flange's inner face and the horizontal on its outer face, all three bolted directly, no plate](../images/structure-tools/tool-lay-out-joints-both-faces.png)
- **Bolts that join nothing** (since 2026-11-21, L10): a member left as it is that bolts onto nothing at a joint (two pieces in line not confirmed one part, their splice not given) is drawn with its bolts — its own, or its design data's —, which then join nothing. Such a joint is no longer counted as laid out whole: the window says *its bolts join nothing* with the member, and why. Confirm the pieces one part (*Pieces in line*), give the splice, or say what it bolts onto. **Since 2026-11-22 (L14)**, where the layout puts a plate on that member's own flange (where nothing runs through the joint: an arm tip, a K-brace's apex), the member's end is bolted onto that plate and its bolts hold it: the notes say *its end bolts hold the plate*, the file names the plate's holder (`held_by`, structure file version 24) and the joint checks check those bolts as the plate's holding bolts. Where its bolts would run into other steel, it is put back as it was (by hand, said with what it runs into).
- **A plate drawn on its flange** (since 2026-11-21, L10; every file): a plate without an outline given, on an angle's flange, all of whose members lie within 2° of that flange, is drawn in the flange's plane (before, in its first member's plane, up to 2° off). An end's own bolt head and nut on its own plate, its leg within 2° of the plate, touch the plate (as on a flange) — the owner's 2° rule; before, they showed as running a few millimetres into it, and the layout took such ends back.
- **An end moved sideways to pass the flange on a plate** (since 2026-11-21, L11, in active development; the owner at a leg splice: "the diagonals need a horizontal shift and a pull back as well as a plate"): an end going onto a plate whose bolts would need more than the longest pull-back to pass the flange's edge is moved sideways — the least whole millimetre within the shift limit, its `shift` (an eccentricity the analysis takes) — so that they pass it within the pull-back; kept at a joint only where it lays out more ends, or as many on fewer plates. The joint then often takes one plate per plane of the leg with every end of that plane bolted onto it. **Tried by [Solve the clashes](#solve-the-clashes) per joint, not in a run of Lay out joints** (measured on real towers: in a whole run it laid out fewer ends at more joints than it gained).
- **One plate per plane** (since 2026-11-21, L11): two plates the layout would put on flanges lying in one plane of members running on as one part through the joint (a chord whose pieces have their own names) are one plate, its ends shared.
- **Both faces of one plate** (since 2026-11-21, L11; the owner's picture: "both diagonals could be on the same plate just bolted on different sides"): an end the layout would put on a plate bent or welded in its own plane, whose leg lies flat (within 2°) in the plane of a flange on that flange's inner side and leaves over the heel, goes onto the **back** of the flat plate on that flange's outer face instead, past the heel — pulled back until its steel clears the heel's line by the gap (its leg's whole width, on the safe side) and its bolts by their nuts and the gap. One plate instead of two or three. The joint details draw such a member against the plate's back (a plate with members on both sides of its leg's flange; before L11 such a member, given by hand, was drawn the flange's thickness and the plate's off the plate).
- A **welded plate** (a plate in a member's own plane welded to a flat plate) is two plates of the file, the one in the member's plane marked in the record so the member is drawn lying on it; its weld's throat and length are written (`[[plate.weld]]`) so the weld is checked — change them like any weld (Joint details › the plate). Since 2026-11-25 (J7b) the layout also writes its **sides**, `sides = 2` (welded from both sides, as laid out; the length both together): where two such plates are welded to one flat plate and a weld is bent about its seam by the other member's force, the two fillets take that moment as a couple across the plate and the check covers it (before: said NOT ON THE SAFE SIDE). A weld you gave by hand on such a plate keeps your sides (not given: the worse of one fillet and two is checked).
- A **bent plate** (since 2026-11-19): a member lying on a flange's inner face that leaves the joint over the heel — through the angle's other leg — is laid out on a plate folded round that leg: one flat where the member lies, the other on the other leg's outer face, bolted to it by two **provisional** bolts (`[plate.leg_bolts]`). The member stays where it lay, only pulled back; the plate is as thick as the thickest leg on it, bent at an inside radius of **two thicknesses — the program's starting value, not a standard's: check it against your steel's bending rules** (change the bend's radius in the file; it is kept). Turning the member half a turn onto the outer face is tried first. The bolts holding the plate lie on the steel of the member they bolt onto (since 2026-11-20): where it ends at the joint, past its cut by the end distance (moved out along it where needed, the flat with them), and slid along it clear of the holes there — the default minimum spacing from each (since 2026-11-20); no place within the longest pull-back: a hand detail, said. Where the other leg is too narrow for the bolts holding the plate, or a member already bolts directly on that leg's face there, the joint says why and the member needs a hand detail. Each flat is checked as a plate; the bend's own bending is checked by the code's plugin (since 2026-11-19; a plugin without that check: not checked, **NOT ON THE SAFE SIDE where it governs** — a failure there would not be found; a plugin with the bend check takes it — said on the plate). In the bill of materials the bent flat is weighed from the developed area.

  ![Lay out joints on an invented joint: a plan member on the inner face of a horizontal's flange, leaving over the heel, laid out onto a bent plate round the horizontal's other leg (two plates, no clash after)](../images/structure-tools/tool-lay-out-joints-bent-plate.png)
- A **plate bent by a few degrees** (since 2026-11-20, L3): written as a bent plate (`[plate.bend]`, its angle the member's few degrees, the inside radius two thicknesses — the program's starting value, check it against your steel's bending rules), the flap carrying the member, the flat on the flange the face's other members. Its bend is a row of the joint checks (the code's strict check: no relief for the flats bearing on each other); the member lies at most one plate thickness farther out of (or nearer) the flange than on a flat plate — like every gusset's offset, not in the analysis. Since 2026-11-26 (L28, in active development): where no plate welded in the member's own plane fits, a plate bent once may put the member up to 40 mm farther out of (or nearer) the flange — the offset recorded with the plate and carried into the analysis as the welded plate's (*Estimated eccentricities*, level 3; otherwise the run says NOT ON THE SAFE SIDE with how many such plates and the largest offset). Since 2026-11-20 (L3b, in active development) also: an equal-leg angle whose bent end has its outstanding leg towards the flange is tried with its other leg bolted (a quarter turn; the end lying flat stays flat), kept only where more of its ends are laid out; and an end that runs back towards the bend going away from the joint is bent away from the flange too, its steel kept clear of the bend wherever it lies over the flange (past the flange's toe it may pass the bend's plane; the clash check checks it).

  ![Lay out joints on an invented K-brace: the legs turned 8°, the diagonal D1 about 5° off the leg's flange, laid out onto a plate bent by as much (G2) from the flat plate on the leg (G1); the close-up turned to look along the leg](../images/structure-tools/tool-lay-out-joints-small-bend.png)
- **An end moved to bolt directly** (since 2026-11-20, L5, in active development): where a face of a member would go onto a plate because some end's bolts do not land on the flange, each such end is tried pulled back further, else moved sideways by at most the plate's thickness (the most is said with it), until its bolts land; where every end of the face then lands, they bolt directly and no plate is made. The move is the end's `shift`: the analysis takes it at level 4, and at level 3 with *Estimated eccentricities* on — without that option it is not in the analysis (**NOT ON THE SAFE SIDE**, as every shift: its moment is missing, so a usage can be too low). It is kept only where the joint lays out more ends, or as many on fewer plates. The window's notes name each end moved and by how much.

  ![Lay out joints on an invented K-brace whose leg, a wide angle, runs through the joint: before, the two diagonals running into each other; after, both bolted directly onto the leg — D1 at the work point, D2 pulled back further clear of it — where without the moves both went onto a plate](../images/structure-tools/tool-lay-out-joints-moved.png)
- A plate **stopping short of the work point** (since 2026-11-20) is marked `short` in the record and its outline written; change it like any outline (it is then yours).
- An `onto` you gave keeps being yours when the layout puts the member onto a plate on that member (since 2026-11-20): Joint details' *laid out automatically* says "onto (yours: D4, kept)", and a run again keeps it.
- **Leg splices**: where two legs of one section meet with their splice bolts, the joint says *where the splice is is not known* — confirm the leg splice places (*Leg splices*: one part by the owner's rule), or give the splice there (+ Splice in Joint details), and lay it out again. A splice you confirmed is yours: a run (also one replacing values given by hand) keeps it and its provisional bolts.
- **Pieces in line confirmed one part** whose rolls were a little apart (Orient angles turns each piece to its face on its own: up to about a degree on real towers) get one roll, half-way between theirs (at most 2° apart — the owner's value of 2026-11-19, not a standard's); the proposal says so. A piece confirmed onto pieces already one part: the part's pieces get the one roll too (the mean of them all), so that no part confirmed before is left turned apart. Orient angles run again may turn them apart (the joint checks then say they are not one part, and each ends at the joint: on the safe side) — confirm them again.
- **The joints left after a run**: **Design › Review the joints left…** lists every joint not laid out whole with why, grouped by reason, to go through them one by one in the Detail view and tick each one reviewed ([Review the joints left](joint-review.md), 2026-11-26).
- **Undo** takes the whole run back in one step.

**What it changes in the results** (see *Usage before and after* in the
window for your tower):

- The **cuts** and **plates** change no member force: the analysis runs on
  the members' axes as before. The plates get their own checks (bearing,
  block shear, section, welds — [plates](../theory/joint-plates.md)); a
  welded plate's weld with the throat and length the layout wrote; a
  splice's covers as plates for their share by area, the bolts with the
  shear planes the covers give.
- A **welded plate** puts its member at most one plate thickness farther
  out of the flange than a flat plate would; that joint eccentricity, like
  a gusset's own, is not in the analysis (an assumption, said in the
  [theory](../theory/joint-layout.md#a-welded-plate-in-the-members-own-plane-step-4)).
- A **bent plate** leaves its member where it lay on the flange's inner face: nothing new for the analysis (its offset in the estimated eccentricities is 0; such an end is estimated, no longer an upper bound).
- A **concentric** end changes nothing in the analysis (drawn with its
  gauge line through the work point; the code's curve already takes that
  eccentricity).
- A **shift** is a rigid end offset: the moments where a frame member
  meets the joint change, and so the usages; where only bars meet it, the
  shifted member's own check takes the moment (since 2026-11-18; a
  cable's is left out, said **NOT ON THE SAFE SIDE**: the members there
  carry more than computed; make it a frame joint).
- **Provisional bolts** do not enter the calculation of the joint details:
  the tower stays at level 3 until no bolt is provisional.
- At level 3 the option **Estimated eccentricities** (Results) takes the
  shifts the layout writes — and, for the joints not laid out, those it
  would give, or the shift limit as an upper bound — without writing
  anything ([user guide](structure-analysis.md),
  [theory](../theory/estimated-eccentricities.md)); a welded plate's
  offset is taken there too.

## Solve the clashes

![The Solve the clashes window on an invented joint: the clashes before and after, what the solution changes (steel stopping short, a plate added), the joint's close-up before and after, Apply and Cancel](../images/structure-tools/tool-solve-the-clashes.png)

(2026-11-21, O44 L11; built, in active development.) **What it is for:**
one click that clears a joint's clashes and problems as a whole, instead
of fixing them one by one. Joint details lists a joint's problems in
**Problems (n)**, closed until you open it, grouped by kind with their
counts (clashes of steel into steel, of a plate, of a bolt's head, nut or
shank; a bolted face off what it bolts onto; an outstanding leg through
the leg it bolts onto; nested angles that do not fit; a cut given that
leaves too little; other), each problem said once. Beside it, **Solve the
clashes…** opens this window.

**How to use it:**

1. In Joint details, pick the joint; beside *Problems (n)* the counts by
   kind.
2. Click **Solve the clashes…**. The window works out the solution on
   copies of the structure (a few seconds on a large tower; "Working out
   the solution…" meanwhile). Nothing is written yet.
3. Read the result: the clashes and other problems **before → after**,
   which way was chosen, **What it changes** (each change at the joint,
   the marked ones in the warning colour), the joint's close-up **before
   and after** (drag to turn both), and on request the members' **usage
   before and after** for the last run's cases.
4. **Apply** writes it as one undo step; **Cancel** or **Escape** closes
   the window, nothing changed.

**What it tries**, each on a copy, in this order:

| Way | What it does |
|---|---|
| The layout of the joint | [Lay out joints](#lay-out-joints) on this joint alone, values given by hand kept |
| … values given by hand replaced | the same, your cuts, shifts, rolls and *onto* replaced by the layout's |
| The layout's proposals here confirmed, then the layout | where the layout proposes pieces in line one part or a leg splice place at this joint, those confirmed first (**marked**: never silent), then the layout (both ways) |
| … ends moved sideways to pass the flange on a plate | where the layout leaves something, the same with an end on a plate moved sideways within the shift limit so that its bolts pass the flange within the longest pull-back (its shift: an eccentricity the analysis takes) |
| Then the fixes of what is left | on the best so far, the fixes of the problems left (the other face, a turn, the gauge or end distance that keeps the code's distances, nesting the other way, lapping instead of nesting), one at a time, each kept only where the joint has fewer problems after it |

The best is the one leaving the **fewest clashes**, then the fewest other
problems, then the fewest members left for a hand detail. A way is
**refused** (said under *Everything tried*) where it:

- pulls back a **leg** or a member **running on through the joint** (one
  part, or a piece in line with another there);
- **turns a leg** (only proposed, below), changes a member's **section** or
  moves its ends;
- leaves more clashes and problems at the **other joints** of the members
  meeting here (a member the layout turns turns at its other end too).

Where nothing tried is better, the window says so and why: each way's
clashes and problems, and what blocks the layout (the reasons it leaves
members for a hand detail).

**What it changes, marked** (in the warning colour): a member **turned**
about its axis (its other end and its axes for the analysis follow), an
end **moved sideways** (an eccentricity the analysis takes — see *What it
changes in the results* above), a proposal of the layout **confirmed**.
Unmarked: where a member's steel stops, what it bolts onto, plates added
or taken away. A member **left** for a hand detail is said with why.

**Good to know:**

- It solves **this joint** (the layout of it alone, not its similar
  joints: [Lay out joints](#lay-out-joints) does those).
- It never changes a member's size, a work point or a leg's turn on its
  own. A leg piece its joints say is turned is only **proposed** (see
  *Leg turned against its joints* below).
- **Undo** takes the whole solution back in one step.

## Solve the clashes at the joints left after the run

(2026-11-26, O44 L30; built, in active development, not approved yet.)
**What it is for:** after a run, the joints it could not lay out whole
get [Solve the clashes](#solve-the-clashes) without you opening each one
— so that what is left for you to check is fewer joints, each with why
([below](#the-joints-left-by-reason)).

**How to use it:**

1. In [Lay out joints](#lay-out-joints), tick **Solve the clashes at the
   joints left after the run** (off by default).
2. Run as usual (the preview). The notes say, for each joint solved, the
   way taken, the clashes, other problems and members left before →
   after, and — marked **MARKED (it turns or moves steel: check it)** —
   every member it turned and every end it moved sideways (an
   eccentricity the analysis takes). The last note counts the joints
   tried, solved, now whole, and the time.
3. **Apply** as usual: one undo step for the run and the pass.

**What it does, and does not do:**

- At each joint the run left not whole it tries what the Solve the
  clashes window tries — but it **never confirms** a proposal (pieces in
  line one part, a leg splice: yours to decide) and never proposes a leg's
  or chord's turn.
- A solution is kept only where the joint is **better on every count**
  (no more clashes, other problems or members left, and fewer of one)
  and **every joint around** (the other joints of its members) has no
  more clashes and problems, joint by joint.
- It changes **no value given by hand** (unless you tick *Replace values
  given by hand*, as for the run), no member's size, no work point, no
  leg's roll, and no roll a weighing of the run set.
- A joint it only fixes (its clashes fewer, the members left still left)
  stays **not whole**: those members are still for a hand detail, with
  the run's reason.
- A run again on the result gives the same file.
- **Time:** slow — measured on real towers, about 20 s per joint left on a
  lightly loaded machine and 80 to 110 s on a busy one; a large tower
  leaves 270 to 340 joints, so the pass takes hours there. Tick it for a
  whole tower when you can wait, or run it in a batch from the command line
  (below).
- **What to expect** (measured on real towers): it clears about half of
  the clashes at the joints it solves (about seven joints in ten), but
  rarely makes a joint whole — most joints are left for reasons a clash
  solution does not touch (a leg off its flange, bolts joining nothing).
  Check those [by reason](#the-joints-left-by-reason).

## The joints left, by reason

(2026-11-26, O44 L30; built, in active development, not approved yet.)
Every member end the layout leaves for a hand detail has its reason; the
reasons are sorted into fifteen classes, each with what to look at (the
[theory's table](../theory/joint-layout.md#the-joints-left-by-reason)):
a clash after laying out, what it bolts onto not fitting after, its leg
off the flange, its axis out of the flange's face, its outstanding leg
into the flange, the flange's inner face, its bolts off the flange, its
plate running into another part, more pull-back than the longest, too
short for its bolts, a member left losing a bolt, its bolts joining
nothing, a value given by hand, a form the layout does not make, other.

From the command line, on a laid-out file (`ohl-structure-layout`, a tool
of its own beside `ohl-structure-export`):

```bash
# Lay every joint out, with Solve the clashes after the run, and write the file.
ohl-structure-layout run tower.ohls --sections libraries/sections --bolts libraries/bolts --solve-left -o tower-laid.ohls
# Every joint left, its classes, and the counts per class.
ohl-structure-layout left tower-laid.ohls
# Only one class, with the layout's words.
ohl-structure-layout left tower-laid.ohls --class leg-off-flange --words
# One row per member end left, for a spreadsheet.
ohl-structure-layout left tower-laid.ohls --csv tower-left.csv
```

`left` reads only the file's layout record (no run, no libraries), so it
is quick on any laid-out file. A whole tower with `--solve-left` takes as
long as a run and the pass together (see *Time* above): run it in a batch,
then check the joints class by class.

## Leg or chord turned against its joints

(2026-11-21, O44 L11; chords and the ticks 2026-11-22, L13; built, in
active development; the owner: "sometimes the main legs are rotated along
their axis 5° out of what would be needed for the joints".) **What it is
for:** finding a leg piece — or a chord running on through a joint (an
arm's chord, a body's horizontal or diagonal running on) — that is turned
about its axis against the members bolting onto it, from **the joints
themselves**. A bracing end can lie flat on a flange only when its axis
lies in that flange's plane; the plane through the piece's axis and the
end's says how far the flange is turned from it. Where at least two
members' ends at a piece's joints say the same turn (all the same way,
within 4° of each other) and it is more than the 2° a member bolts flat
within, the piece is flagged with that turn; the pieces in line with it
turned alike are named with it (one leg or chord). For a leg, the tower's
faces through the neighbouring legs (what Orient angles turns a leg to)
are said beside it as a hint only — never flagged on their own.

**Beside it, from the joints' planes** (2026-11-23, O44 L18; built, in
active development): the whole leg or chord in line (its pieces in line,
turned alike — one roll along its length) turned so that the most member
ends bolting onto it lie within 2° of its flanges' planes **at all its
joints**, centred in the turns that keep them so, at most 22.5°
([Orient angles](#orient-angles) does the same, automatically, on a tower
it orients). The two proposals are offered **side by side**, each with its
own tick; one turn per member (ticking one clears the other kind's tick of
the same pieces). **A turn is never judged on one joint**: every joint the
turned members reach (both ends of each piece, every joint along the run)
is laid out with it and its effect counted there. **The turns Orient
angles set from the planes are weighed by the layout run** (2026-11-23,
O44 L19; built, in active development): the run lays the tower out with
every such turn taken back, then with the legs' and chords' turns, then
the face members', keeps each only where every joint it reaches is laid
out no worse and one better, and puts the others' rolls back — said in
the preview's notes ("leg run … turned by …: kept — its *n* joint(s) lay
out … (before …), none worse" or "… taken back (joint(s) … laid out worse
with it …); offered in Lay out joints"), and offered here beside the
other proposals (its tick as for any plane turn). A turn is weighed only
by a run that lays out every joint it reaches (the whole tower: Lay out
joints weighs once when it works out its preview, and lays out every joint
a turn kept reaches); a run on a few joints leaves it as written for
later. **Time:** the weighing lays the tower out about five and a half
times (measured on six real towers: median 45 and 71 s unweighed, 266 and
335 s weighed) — once after Orient angles: the file written lists nothing
more to weigh.

**An outstanding leg pointing into the flange** (2026-11-25, O44 L25;
built, in active development): a member end left because its outstanding
leg points into the flange (a plate in its own plane would stand on the
flat plate) is laid out with its member turned by half a turn and by a
quarter turn either way (the quarter turn bolts its other leg; on an
unequal angle only where no leg is given), each weighed over **every
joint the member reaches** as above; the one laying out more member ends
is kept, on a tie the half turn. Where the member's roll is the
program's (an import's) the turn is applied and written in the layout
record, said in the notes ("… turned half a turn … — kept, its joints …
lay out … (before …), none worse"); where its roll is given by hand it is
**offered** here, under *Members with an outstanding leg pointing into the
flange*, with its effect — tick it to apply it with the run. Weighed once
per tower (Lay out joints weighs when it works out its preview); a member
weighed is not weighed again until its roll changes.

**The turns the far-out welded plates change** (2026-11-26, O44 L28;
built, in active development, not approved yet): a member end may go onto
a plate in its own plane welded to the flat plate that puts its axis up
to 40 mm farther out (the offset recorded for the analysis), and the
run's own half and quarter turns and its turns onto a flange's other face
are chosen with such plates counted. Before L25's turns the run lays the
tower out once more with those turns chosen without them; each member
whose roll that changes is decided over **every joint it reaches** as
above — the roll chosen with the plates kept only where every such joint
is laid out no worse and one better, else (a tie too) the roll chosen
without them, written and kept by later runs. The preview's notes say
each member, both rolls, its joints' ends and plates either way and which
was kept and why ("… the roll without them kept (joint(s) … laid out
worse with them); held by later runs"); the layout record lists them
(`own_turns`), so a run again decides nothing more. Members written one
part with another (`continuous`) are decided as a
whole part at once (since 2026-11-27, O44 L32, in active development):
over every joint any of them reaches, every member at its roll with the
plates or every member at its roll without them, never apart — the notes
say the part ("…, …: one part, decided at once …"), and where the run
writes the pieces' rolls more than 2° apart, that they are not alike.
Where the roll is
given by hand the run turns nothing, so nothing is decided. **Time:** one
layout of the tower more, and one or two more where a member is decided
(said in the notes, measured).

**Where it shows:**

- **Lay out joints**: under the numbers, "Legs and chords turned against
  their joints, two ways side by side", then one line per proposal: "Leg
  (from the ends at its joints): …" or "Chord …" with a **tick, off by
  default**; "Leg (from its joints' planes; …): leg run … turned by …"
  with a tick **on by default where its roll is the program's** (set by
  Orient angles or the layout — an import's: automatic, untick to keep it)
  **and every joint it reaches is laid out better with it** (at none fewer
  ends nor as many on more plates; in all more ends, or as many on fewer
  plates), **off where its roll was given by hand** (a designed tower: a
  proposal) or where its joints are not laid out better (said). Each line ends with its effect **at the joints it reaches**
  (whole, member ends, plates without and with it — all the proposals of
  its kind applied together, measured by the layout run both ways). With
  turns ticked the preview lays out every joint they reach with them and
  says the run **with and without** them. **Apply** then writes the turns
  with the layout, one undo step.

  ![Lay out joints on an invented K-brace whose chord is turned 5° against its joint: the chord's turn proposed with its tick, ticked, and the run with and without it](../images/structure-tools/tool-lay-out-joints-turns.png)
- **Solve the clashes**: under *Proposed, not applied*, "Turn L2, L1 by
  −5.00° (from the ends at its joints (L11)): here clashes … to …, other
  problems … to …; at the *n* joint(s) the turned members reach: clashes
  and problems … to …, … laid out whole, … member end(s)" and the same
  "(from its joints' planes (L18))" — each the joint's solution worked
  out again with every joint the turned members reach laid out with the
  turn; a turn leaving those joints with more clashes and problems is
  refused. **Tick one** to apply that solution instead (one undo step);
  unticked, nothing of it changes.

**Good to know:** a leg's or chord's turn is never changed silently, by
the layout or by Solve the clashes (the run's own chord turns, a setting,
stay off); a roll given by hand is only ever proposed. Where Orient angles has not been run on an imported tower,
most legs show as turned (an import carries no member orientation): run
Orient angles first ([below](#orient-angles)); the check then shows the
pieces Orient angles cannot place (where no faces are found) or the joints
disagree with. Where a chord's two faces meet off square (an arm's faces a
few degrees from 90°), Orient angles leaves each flange half the
difference off its face; the joints may then ask for one flange in the face
their ends bolt onto — turning it lays that face flat and the other twice
as far off, which the tick lets you weigh.

## A joint's group

(2026-11-21, O44 L12; built, in active development.) **What it is for:**
understanding why a joint is or is not laid out. **Joint details** shows,
under the joint's member count, a small line "Group: …" in three parts:

- **what the other ends bolt onto** — a leg running on, a leg splice, a leg
  ending here, a built-up leg, a chord running on (an arm or the top), a
  member running on (the body), a crossing, nothing running through, a
  built-up member;
- **what comes in** — redundants, bracing, or with a plan (level) member;
- **how the ends lie against that member's flanges** — all in a flange's
  plane (within 2°: they can lie flat on it), some a few degrees out (a
  sloping face), some out of plane (a plan member at a leg, a member
  between two faces).

The hover names the member they bolt onto. Nothing depends on it: the
layout's results are the same with or without it. On the private towers
measured, a joint whose ends all lie in a flange's plane is laid out
whole four times in five; one with an end out of plane seldom — the
reason is then usually that end (its leg cannot lie flat there), whatever
the joint's place in the tower.

## Apply this joint's detail to similar joints

![The Apply this joint's detail to similar joints window: the joints like J10 of an invented body, each turned a quarter turn, with what changes there, ticked](../images/structure-tools/tool-apply-detail.png)

(2026-11-19, J4; built, in active development, not approved yet.) **What
it is for:** a tower repeats its joints — its symmetry (mirror planes,
quarter turns about its axis) and its repeated panels. Once one joint is
detailed — laid out by **Lay out joints** or detailed by hand — this
copies that detail, mirrored or turned, onto the joints like it, so they
are made alike.

**How to use it:**

1. In **Joint details**, show the joint whose detail you want to copy.
2. Click **Apply this joint's detail to similar joints…** in the joint's
   bar (beside *Copy to the joints like it*).
3. Read the **preview**, worked out on a copy: each joint like it, how
   (mirrored across x or y or a diagonal, turned a quarter, a half or
   three quarters, or the same moved: a repeated panel), how many member
   ends get this joint's connections and how many plates it has before and
   after; the joints left as they are, with why.
4. Untick a joint to leave it as it is.
5. **Apply**: one undo step. Escape or Cancel: nothing changes.

**The window:**

| Field | What it does |
|---|---|
| The joints it changes | each joint like this one, ticked: changed. A joint is *like* this one when a move with a mirror or a quarter, half or three-quarter turn about the vertical carries every member end here onto one of it: the same direction and length (within a tenth of a degree and 2 mm — the tool's tolerance, not a standard's), the same cross-section turned alike, the same class |
| Left as they are | a joint with a splice (here or there: its covers are copied by hand); a joint where the copy would run into more than it does here (the clash check in its own place) |
| **Apply (n joints)** | one undo step; greyed while the preview is worked out or when no joint is ticked. **Enter** applies |
| Cancel, Escape | nothing changes |

**Good to know:**

- At each joint changed, its own connections and plates are **replaced**
  by this joint's: what an end bolts onto said with that joint's members
  and its new plates, the plates' outlines and bend lines placed there,
  the cuts, shifts and bolts as here (a shift is measured towards what the
  end bolts onto, the bolts along and across a leg: the same on a mirrored
  joint).
- A **laid-out** detail stays *automatic* there (a later run or edit may
  lay it out again); one **given by hand** stays yours; a joint left for a
  hand detail is copied with the same words, said with its own members.
- **Lay out joints** does this by itself: it plans one joint of each kind
  and applies that plan to the joints like it — checked for clashes in
  each one's own place, a joint where it does not fit planned on its own
  ([theory](../theory/joint-layout.md#laid-out-once-applied-to-the-joints-like-it-j4)).
- The joints are matched by their members only, not by their connections:
  a joint like this one with other bolts or plates gets this one's.
- **Undo** takes the whole copy back in one step.

## Orient angles

![The Orient angles window: which members, rolls given by hand kept or replaced, the preview with what turns and the legs left](../images/structure-tools/tool-orient-angles.png)

(2026-11-18; built, in active development.) **What it is for:** a tower
whose members carry no orientation — an import (the export a tower is
imported from carries no member orientation, so every angle takes the
default and its legs stand turned anyhow), a tower drawn by hand — gets
its legs and chords turned as a tower is built, before **Lay out
joints**.

**Setting it up**, in order:

1. Give the legs the class `leg` (Member › design data; an import has it)
   and the members their angle sections.
2. Open **Design › Orient angles…** (the members picked, else the whole
   tower).
3. Choose **which members**, how the legs and chords are turned (**from
   their joints' planes**, the default, or from the tower's outline, as
   before) and whether **rolls given by hand** are replaced (an import's
   rolls: replace them).
4. Read the **preview**: what turns, the rolls kept, the legs left and why.
5. **Apply**: one undo step. Then **Lay out joints**.

**What it turns:**

| Members | How |
|---|---|
| **Legs and chords**: the members of class `leg`, and the angles running on in line (within 10°) through a joint | heel out of the tower, each flange in one of the two faces meeting at it. The faces are those of its neighbouring legs' lines: the tower's section through the member's middle (a leg's from the other legs, a chord's from the other chords: those within 70° of it whose lines a member joins to its line, lying in one plane with both — a face's bracing, a plan member; not a portal's other column, not an arm joined to a leg) is a polygon, the member one of its corners, the corners next to it its neighbours. An angle's legs are at 90°, the faces seldom exactly: the middle of the angle's legs is put on the middle of the faces, each flange off its face by half the difference (said; more than 2° and the bracing there cannot lie flat on it: a bent plate or a hand detail); faces meeting at less than 45° or more than 135° are no corner for an angle (left, said). Two angles in a star or four in a cross are turned the same way; two back to back are left |
| **From the joints' planes** (the default since 2026-11-23, O44 L18) | after the outline's turn, each leg's run in line (pieces in line within 10°, turned alike within 2°: one roll), then each chord's, then each face member's is turned about its axis so that **the most member ends at all its joints lie within 2° of its flanges' planes** — where a piece is what others bolt onto, the angle each end's axis runs out of its nearer flange's plane; where it bolts onto another member, its own leg against that flange —, centred in the turns that keep them so (a later small change, pieces confirmed one part with one roll, keeps them in), then the least left over, at most 22.5° from the outline's turn (the heel stays out). The legs the outline leaves (no faces found) are turned from the roll they have, unless it was given by hand and not replaced. Each run is listed with the ends within 2° before and after and, on hover, every end's angle before and after at each joint. **Weighed by the next Lay out joints run** (2026-11-23, O44 L19): each run's turn is recorded as not weighed yet; the run keeps it only where **every joint its members reach** is laid out no worse and one better (and no joint elsewhere is laid out worse where a member the layout turned otherwise beside it meets — a bracing's one roll turned for its end at the turned member), else puts the roll before back and offers the turn ([below](#leg-or-chord-turned-against-its-joints)) |
| **Plan and hip members**: horizontal angles (within 5°) lying in no steep face | one leg flat, the other pointing down (**assumed**: no water lying in it — a choice of this tool, not a standard's), the flat leg towards the tower's middle |
| **Face members**: the other single angles lying in a steep face (within 20 mm and 5 mm per metre of their length of its plane, between its legs or chords) — its diagonals (also those running on through their crossing), horizontals and redundants | one leg in the face, the other pointing out of the tower (on the outer face of the legs' flanges, usual practice); the leg in the face down from the heel where it can (**assumed**: no water lying in it); an unequal angle's leg bolted in the face. **Lay out joints** may turn it half a turn where an end of it does not fit on an inner face |
| **The rest** (members in no face: a sloping crossarm's bottom bracing, diaphragms) | left: **Lay out joints** lays it flat on what it bolts onto, where that helps |

A leg (class `leg`) is **left**, with why, when it is no corner of the
tower's section (inside it, on a face) or has fewer than two neighbouring
legs or chords (a peak of two legs). An angle running on in line that lies
in a face of the legs (a diagonal through its crossing, a horizontal
through a redundant's joint) is a face member, not a chord.

**The window:**

| Field | What it does |
|---|---|
| Members | the members picked · a member group's · the whole tower (the faces are always found from all the tower's legs and chords) |
| Legs and chords turned | **from their joints' planes** (default) · from the tower's outline (as before, for comparison) |
| Replace rolls given by hand | off: a member's `roll`, `reference` or `orientation` given by hand is kept (listed with the turn it would get); on: replaced — for an import whose rolls are not as built |
| Preview | how many members turn (legs and chords, plan members), those already so, the rolls kept, the legs left with why, the time it took (measured); from the joints' planes: the runs turned and the member ends within 2° before and after; **What turns**: each member's turn and its roll before and after |
| **Apply (n members)** | one undo step; laid-out joints at the members turned are laid out again in the same step |
| Cancel, Escape | nothing changes |

What it set is **automatic** (the record's `oriented`, with each roll
before): a run again replaces it, a roll you change becomes yours. A plan
or face member's roll that **Lay out joints** chose for both its ends is
kept (2026-11-19, J3; listed in the preview, *Rolls the joint layout chose
for both ends kept*): the two agree, a run of either after the other
changes nothing. **Lay
out joints** keeps a leg's or chord's roll set here (the tower's shape)
and may turn a plan or face member's where that helps.

**What it changes in the results:** a member's roll turns its section's
axes: the members' stiffness about their axes and so the forces of a
frame model change a little, and the code check takes the angle's axes
as turned (the slenderness of a single angle about its weak axis does not
change with the roll).

