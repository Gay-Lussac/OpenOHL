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
   `redundant`…) first: the layout works from them.
2. Open **Design › Lay out joints…** (the joints picked, else the whole
   tower), or **Lay out joint…** in Joint details (that joint).
3. Choose **which joints**: the joint shown, the joints picked, a member
   group's joints or the whole tower; tick **With their similar and
   symmetric joints** to lay those out alike.
4. Check the **settings** (each one's effect shows at once in the
   preview below): the gap between steel, the longest pull-back, the shift
   limit, the bolt for ends without bolts (and a connection preset per
   member class), the plates of a corner bolted to their flanges or welded
   together, and whether values given by hand are replaced.
5. Read the **preview**: how many joints are laid out, how many plates,
   the joints that need a hand detail and why, the clashes at these joints
   before and after, the time it took (measured); **What changes** per
   joint; a joint's close-up **before** and **after** (drag to turn both);
   **Usage before and after** (the members' largest usage over the last
   run's cases, as the file is and as laid out — the analysis runs twice).

   ![A joint before (D1 and H1 running into each other, red) and after (pulled back onto plate G1, no clash)](../images/structure-tools/tool-lay-out-joints-before-after.png)
6. **Apply**: one undo step. Escape or Cancel: nothing changes.

**How the layout places a member end** ([theory](../theory/joint-layout.md)):

| Step | What happens |
|---|---|
| Who arrives | each member end bolting by one leg of an angle onto the flange of a member at the joint (the leg, a chord, a diagonal a redundant meets); members in line with it (a splice, one part) are left alone; an end bolting onto nothing gets the member running through the joint |
| Laid flat | a member lying more than 5° off what it bolts onto is turned (its roll) — where its two ends agree within 5° (the middle of their turns); a roll given by hand is kept unless replaced |
| Concentric first | its gauge line through the work point (drawn so; the analysis keeps the member's axis there, the code's slenderness curve takes the eccentricity gauge to centroid); its end pulled back until its steel clears the flange's heel (on the inner face: the angle's other leg and root) and the ends placed before it by the gap — bracing before redundants, the wider leg first, the larger angle first |
| A shift | only where that needs more than the longest pull-back (or more than the member's half less its bolts): the smallest sideways move that fits, a millimetre at a time, up to the shift limit. The analysis takes it as an eccentricity (a rigid end offset) |
| Onto the leg or a plate | directly when every bolt of the plane lands on the flange (clear of its other leg and root by the nut, the edge distance from its toe — the code's when its plugin gives one, else the structure's default minimum, assumed, not a code's); else every member of the plane onto one plate on that flange, its bolts past the flange by the nut and the gap. Plates: as thick as the thickest leg bolted on it, outline round the bolts (the code's end and edge distances) cut at the flange's heel by half the gap; at a corner one per face, bolted to its flange, or welded together |
| No fit | the member is left exactly as it is and listed with why ("R1 and D1 within 3°: no room on one plate …"); the rest of the joint is laid out. A joint whose laid-out parts would still run into anything is left as it was, with the clash. Never forced |

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
| **Apply (n joints)** | writes the preview, one undo step; greyed while the preview is worked out |
| Cancel, Escape | closes the window, nothing changes |

**Good to know:**

- What the layout set is **automatic** (the file's [record](structure-library.md#the-joint-layouts-record)): Joint details says *laid out automatically* (on hover: which values), *provisional bolts* and *Needs a hand detail: …*. A value you change becomes yours and is kept.
- **It follows later changes**: an edit near a laid-out joint — a plate made thicker, a member's section, roll or bolts, a value at an end, a joint moved, a setting — lays that joint out again in the same undo step, your values kept, the bolts' grips following the plates. If it no longer fits, the edit is kept, the joint's layout stays as it was and the edit's message says *no longer fits, its layout kept, needs a hand detail* with why.
- **Provisional bolts keep the tower at level 3** for the joint details — everywhere: OHL-Structure, OHL-Design (the structure picker and the structure's properties say why), `ohl structures fe` and the line's calculation level 4. With `joint_details = true` the calculation says so and checks the members in the simple form until no bolt is provisional.
- Where the design code gives **no minimum bolt distances** (the simple check, a plugin without them), the layout keeps the structure's **default minimums** (Design › Analysis, *default bolt minimums*: end and edge 1.5, spacing 3 bolt diameters to start with) — a default minimum, assumed, not a code's. The provisional bolts' pitch and end distance are the default minimums too.
- A member on the **inner face** of a leg (as its roll turns it) is laid out there when its bolts land on the flange; no plate is laid out on an inner face (turn the member, or lay it out by hand). A plate on a **built-up** leg is not laid out (drawn through the work point). Members on both sides of one plate: by hand only.
- **Undo** takes the whole run back in one step.

**What it changes in the results** (see *Usage before and after* in the
window for your tower):

- The **cuts** and **plates** change no member force: the analysis runs on
  the members' axes as before. The plates get their own checks (bearing,
  block shear, section, welds — [plates](../theory/joint-plates.md)).
- A **concentric** end changes nothing in the analysis (drawn with its
  gauge line through the work point; the code's curve already takes that
  eccentricity).
- A **shift** is a rigid end offset: the moments where a frame member
  meets the joint change, and so the usages; where only bars meet it, the
  offset is left out and said (**NOT ON THE SAFE SIDE** there).
- **Provisional bolts** do not enter the calculation of the joint details:
  the tower stays at level 3 until no bolt is provisional.
