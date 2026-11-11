# Staying on the safe side (audit, 2026-11-08)

The owner's rule (2026-11-08): *"what's important to me is that we stay on
the safe side and that it's explained why and where it has to be done so."*

Every calculation is **conservative**. Where one cannot be — a value only
a standard or the user can give, a clause not covered, a limit of the
method — the **result says so where it applies** (on the member, the pole,
the structure, the span: "not on the safe side" and why), and the theory
page explains why and what the exact treatment would be. A part that is
not checked is never shown as passing.

This page lists every place the theory pages called a treatment *not on
the safe side* (an audit of 2026-11-08) and what is done about it. Each
change is also in the [decision log](decisions.md) and marked in its
theory page. Status: **done** (with the date), **open**.

## A. Made conservative by a rule or a default

| Where | What was not on the safe side | Done about it | Status |
|---|---|---|---|
| [Joint plates](../theory/joint-plates.md) | A member bolted onto two plates shared its force between them by the bolts on each | Each plate checked for the member's whole force | done 2026-11-08 |
| [ASCE 10-97](../theory/asce10.md), [SIA 263](../theory/sia263.md) | A member end without bolts: its connection not checked, tension on the gross section, the member shown as passing | Every part a check could not do marked `[not on the safe side: …]` on the member: never shown as passing (violet in OHL-Structure, "not all checked" in OHL-Design and the report) | done 2026-11-08 |
| [Structures](../theory/structures-fe.md) | Single angles' end moments not checked by default (as ASCE 10-97 intends: its curves include the connection's eccentricity) | Kept off by default; a single angle whose end moments would add 1 % or more is marked not on the safe side with what they would add | done 2026-11-08 |
| [Structures](../theory/structures-fe.md) | A joint held by the solver (stabilisation) below the refusal limit: its force does not go through the members | Said "NOT ON THE SAFE SIDE" in the notes with the holds and the largest force (above 1 N) | done 2026-11-08 |
| [Laps and splices](../theory/joint-laps-splices.md) | SIA 263 bearing on the leg bolted onto took the member's own e₁ | The smaller of the member's e₁ and Table 17's minimum; no minimum for the bolt: marked not on the safe side | done 2026-11-08 |
| [Wind on structures](../theory/structure-wind.md) | A polygonal shaft's ice on the circle's perimeter πD | The polygon's own perimeter, n D tan(π/n) | done 2026-11-08 |
| [Structures](../theory/structures-fe.md) | Frames checked with their end moments only: the moment in the span from their own weight and wind missed | The largest moment along the member (its end forces and self-weight exactly; loads put on its joints, q L² / 8); axial-only members: said when it would add ≥ 1 % | done 2026-11-08 |
| [Structures](../theory/structures-fe.md) | A bowed (buckled) member keeps carrying its elastic buckling load N_cr, unloading the others | Capped at the lower of N_cr and its design compression resistance | done 2026-11-08 |
| [Joint plates](../theory/joint-plates.md) | Block shear: tearing towards a side edge of the plate not checked | The path towards the side edge checked too (one shear plane, the tension plane to the edge), under both codes | done 2026-11-08 |
| [Wind on structures](../theory/structure-wind.md) | Overlaps counted twice in the solidity ratio (more shelter); gusset plates without wind area | The solidity ratio taken within bounds (overlaps at most once less: a bound by clipping the members' strips; plates added) and the largest face drag and shielding factor over them; the joints' plates and covers in OHL-Structure's wind on the body; where the joints are not known (line checks, `ohl structures fe`), "NOT ON THE SAFE SIDE: gusset plates' own wind area not included" | done 2026-11-08 |
| [Wind on structures](../theory/structure-wind.md) | A face's drag outside the table: the shape's drag or 1 | The table's largest face drag (or the shape's when larger) | done 2026-11-08 |
| [Structures](../theory/structures-fe.md) | Buckling length k = 1 between joints: a diagonal split at a crossing nothing holds out of plane buckles over more | Said on each half in compression through such a crossing (without lk, k or a crossing partner) | done 2026-11-08 |
| [Clearances](../theory/clearances.md) | In a swing band the wire's radius taken off radially, not r / cos β | r / cos β (`SwingArc::vertical_outer`) | done 2026-11-08 |
| [LiDAR](../theory/lidar-analysis.md) | A voltage above the table's last row: that row's distance | Kept (owner, 2026-10-27): checked with the last row's distance, each such span named in the report's warnings as on the unsafe side ("extend the table") | kept (owner) |
| [Thermal](../theory/thermal.md) | Wire-to-wire rules left out of the temperature limit; a 10 °C grid can miss a narrow failure | Said on every limit: the wire-to-wire rules applying to the base case named; with a radial rule, a failure between two scan steps (the step is a setting) | done 2026-11-08 |
| [Jumpers](../theory/jumpers.md) | The riser's ice and wind per unstretched metre | Per stretched metre at the largest stretch (a second solve), the own weight per unstressed metre | done 2026-11-08 |
| [Sag-tension](../theory/sagtension.md#bimetallic-wires-above-the-knee-point) | Bimetallic wires above the knee point: which of the models bounds the hot sag was not said (the roadmap first took the default for the smaller sag; it is the larger) | Kept: the default (outer strands in compression without limit) is the conservative one for the sag — the largest hot sag of the models; said with the wire types and sections on the results (`ohl_line::bimetallic_notes`) | done 2026-11-11 (in active development) |

## B. A value is needed: said on every result until it is given

| Where | Value | Said where it applies | Status |
|---|---|---|---|
| [Structures](../theory/structures-fe.md) | Dead-load factor (bolts, plates in the self-weight) | "NOT ON THE SAFE SIDE: no dead-load factor given" in every run's notes | done 2026-11-08 |
| [Structures](../theory/structures-fe.md), [Wind on structures](../theory/structure-wind.md) | Drag factors of the profiles and faces; a load recipe's pressure without them | Every member wind with members on the generic drag 1 and no `[analysis] drag` given: "NOT ON THE SAFE SIDE: n member(s) take the drag factor 1 …"; each load-table row of the line's wind pressure on a member's bare area | done 2026-11-08 |
| [Wind on structures](../theory/structure-wind.md) | Wind band areas per direction | On each band load of a case blowing with a part along the line (its label) | done 2026-11-08 |
| [Structures](../theory/structures-fe.md) | A pole's modulus E (P-Delta) | In the pole's result ("[not on the safe side: P-Delta not computed …]"), never shown as passing | done 2026-11-08 |
| [Internal clearances](../theory/internal-clearances.md) | A string's volumes | Per structure, the strings without volumes named, in the report's notes | done 2026-11-08 |
| [Thermal](../theory/thermal.md) | Steel-core magnetic heating in the AC resistance | Above every thermal result, the sections of ACSR and ACSS conductors named | done 2026-11-08 |
| [Jumpers](../theory/jumpers.md) | The jumper's bending stiffness (clamp pull) | With the pull option, every load case's notes name the jumpers whose stiffness is estimated | done 2026-11-08 |
| [Sag-tension](../theory/sagtension.md#bimetallic-wires-above-the-knee-point) | The compression a bimetallic wire's outer strands really take above the knee point (tests or the maker) | A wire type as a bird cage or with a compression limit: "NOT ON THE SAFE SIDE: wire type X (sections …) modelled …" on every result of the line (its hot sag is not the largest); each weather case flags the spans past the knee point — the default's for the tension (the lowest of the models, about 1 %), a bird cage's or a limit's for the sag | done 2026-11-11 (in active development) |

| [Sag-tension](../theory/sagtension.md#creep-at-elevated-temperature) | A wire type's law of creep at elevated temperature (its coefficients: the maker's or a published predictor equation's — none is built in; the sources are missing) | A line with a time at high temperature and a wire type without a law: "NOT ON THE SAFE SIDE: wire type X gives no law of creep at elevated temperature — its creep during … is not counted" on every result of its sections after creep; with a law, every such result names its source (the user's coefficients) | done 2026-11-11 (mechanism only, in active development) |

| [Sag-tension](../theory/sagtension.md#gap-type-conductors) | Whether a conductor given as a gap type is installed as one (strung by its core, the outer strands unloaded): the wire file's statement | "NOT ON THE SAFE SIDE: gap-type wire X (sections …): the outer strands taken as unloaded at each section's stringing state … a conductor installed otherwise sags more when hot" on every result of the line (0.79 m less sag at 60 °C than the same wire as an ordinary conductor on the benchmark span) | done 2026-11-11 (mechanism, in active development; its source is missing) |

## C. Limits of the method or clauses not covered: said where they apply

| Where | Limit | Status |
|---|---|---|
| [ASCE 10-97](../theory/asce10.md), [SIA 263](../theory/sia263.md), [ASCE/SEI 48-19](../theory/asce48.md), [SIA 265](../theory/sia265.md) | Clauses not covered: named on the member where the model's data tell they apply (ASCE 10-97 3.9.3, 3.11, 3.12/3.13, 3.14, 3.15; SIA 263 5.5, 5.1.4/5.1.7); the rest (staggered bracing, bolts in tension, a pole's holes, slip joints, base plates, connections, foundations, f_y by thickness) cannot be told from the data and stay said in the theory pages | done 2026-11-08 |
| [SIA 263](../theory/sia263.md) | A pole's buckling as a member (4.5.1) | On every SIA 263 pole's result | done 2026-11-08 |
| [ASCE 10-97](../theory/asce10.md) | Restraint decided once per model; minor-axis I/L; the bolts' centroid condition: said on the member where an end is restrained by the I/L rule or worked out concentric (the gauge is only shown: the curves know eccentric or not; bolts in tension cannot be told from the data, theory only) | done 2026-11-08 |
| [Structures](../theory/structures-fe.md) | Lateral-torsional buckling of frames (not found by the solve): ASCE 10-97 3.14 named on I sections and channels bent about their major axis; SIA 263 checks it (Annex B) | done 2026-11-08 |
| [Structures](../theory/structures-fe.md) | A generic pole check without shear or torsion: said on its result | done 2026-11-08 |
| [Structures](../theory/structures-fe.md) | Factors on load components (either side, by the code's factors) | Kept: the owner's load cases factor the components (as load-tree programs); said in the theory and the load case library's help | kept (owner) |
| [Wind on structures](../theory/structure-wind.md) | Snow bridging: said with every snow on the members (line check notes, each OHL-Structure snow row) | done 2026-11-08 |
| [Wind on structures](../theory/structure-wind.md) | Allowable loads taken to include the body's ice and snow: said on the allowable-loads check of a structure without a model or pole, in a case with ice or snow on the structures | done 2026-11-08 |
| [Wire FE](../theory/wirefe.md) | Structures rigid in the wire solve (sags low for clearances): said with the clearance-zone, internal-clearance and LiDAR reports (the interaction itself is open point O7) | done 2026-11-08 |
| [Structures](../theory/structures-fe.md#a-share-of-the-spans-pull-a-load-assumption) | A structure load case taking a share of the back / ahead spans' solved pull (`back`, `ahead`, percent; owner 2026-11-10): a load assumption, not an equilibrium state — the real one-sided pull with swinging strings can be smaller or larger | Said where it applies: on every load-table row of such line forces ("NOT ON THE SAFE SIDE: …"), in the case's sentence, the solved case's notes, the report and the exports' notes; the exact treatment (a line result solved with the one-sided condition) is built and named there. No count of not-safe load rows exists (the counts are the members' checks) | said where it applies, 2026-11-10 |
| [Wire FE](../theory/wirefe.md#a-constant-force-across-or-along-the-line), [Sag-tension](../theory/sagtension.md#point-and-distributed-loads) | The ruling-span method leaves point loads out (marker balls' weight, a point's constant force): smaller sags, no pull across or along | Said on the section in every weather case it is solved in by that method, the loads named ("NOT ON THE SAFE SIDE: point loads (…) left out by the ruling-span method — use FE"); before 2026-11-11 only in the user guide | done 2026-11-11 (in active development) |
| [Spotting](../theory/spotting.md) | One catenary for every span (the ruling-span idea); holes in the terrain bridged: said in every spotted route's notes | done 2026-11-08 |
| [LiDAR](../theory/lidar-analysis.md) | Leaning trees and crowns: said whenever fall-in is checked | done 2026-11-08 |
| [Thermal](../theory/thermal.md) | Uniform radial temperature; one weather case per section: said above every thermal result | done 2026-11-08 |

Already said in the results where they apply (kept): members without a
profile get no ice; wind-band types no ice or snow; a member's centre
line where it has no profile (internal clearances); an offset's moment
left out at a joint of trusses only; the simple check is no design code;
the wire load per unstretched metre (an option).
