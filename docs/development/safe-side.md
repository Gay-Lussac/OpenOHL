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
| [Joint plates](../theory/joint-plates.md) | Block shear: tearing towards a side edge of the plate not checked | The side paths checked | open (batch 1b) |
| [Wind on structures](../theory/structure-wind.md) | Overlaps counted twice in the shielding; gusset plates without wind area | Overlaps once; the plates' areas in the wind | open (batch 1b) |
| [Wind on structures](../theory/structure-wind.md) | A face's drag outside the table: the shape's drag or 1 | The table's largest face drag (or the shape's when larger) | done 2026-11-08 |
| [Structures](../theory/structures-fe.md) | Buckling length k = 1 between joints: a diagonal split at a crossing nothing holds out of plane buckles over more | Said on each half in compression through such a crossing (without lk, k or a crossing partner) | done 2026-11-08 |
| [Clearances](../theory/clearances.md) | In a swing band the wire's radius taken off radially, not r / cos β | r / cos β | open (batch 3) |
| [LiDAR](../theory/lidar-analysis.md) | A voltage above the table's last row: that row's distance | The span not checked, said | open (batch 3) |
| [Thermal](../theory/thermal.md) | Wire-to-wire rules left out of the temperature limit; a 10 °C grid can miss a narrow failure | Both included; the grid refined below the limit found | open (batch 3) |
| [Jumpers](../theory/jumpers.md) | The riser's ice and wind per unstretched metre | Per stretched metre | open (batch 3) |

## B. A value is needed: said on every result until it is given

| Where | Value | Said where it applies | Status |
|---|---|---|---|
| [Structures](../theory/structures-fe.md) | Dead-load factor (bolts, plates in the self-weight) | "NOT ON THE SAFE SIDE: no dead-load factor given" in every run's notes | done 2026-11-08 |
| [Structures](../theory/structures-fe.md), [Wind on structures](../theory/structure-wind.md) | Drag factors of the profiles and faces; a load recipe's pressure without them | Per structure and per case, "not on the safe side" | open (batch 2) |
| [Wind on structures](../theory/structure-wind.md) | Wind band areas per direction | Per structure on cases blowing ahead or at an angle | open (batch 2) |
| [Structures](../theory/structures-fe.md) | A pole's modulus E (P-Delta) | In the pole's result and report | open (batch 2) |
| [Internal clearances](../theory/internal-clearances.md) | A string's volumes | Per row where a string has none | open (batch 2) |
| [Thermal](../theory/thermal.md) | Steel-core magnetic heating in the AC resistance | Per section of a steel-cored conductor | open (batch 3) |
| [Jumpers](../theory/jumpers.md) | The jumper's bending stiffness (clamp pull) | On the clamp loads while not given | open (batch 3) |

## C. Limits of the method or clauses not covered: said where they apply

| Where | Limit | Status |
|---|---|---|
| [ASCE 10-97](../theory/asce10.md), [SIA 263](../theory/sia263.md), [ASCE/SEI 48-19](../theory/asce48.md), [SIA 265](../theory/sia265.md) | Clauses not covered (effective widths, lateral-torsional buckling, stitch bolts, net section of poles, foundations…): the clause that applies named on the member or pole | open (batch 2) |
| [SIA 263](../theory/sia263.md) | A pole's buckling as a member (4.5.1) | open (batch 2) |
| [ASCE 10-97](../theory/asce10.md) | Restraint decided once per model; minor-axis I/L; the gauge not used; bolts in tension | open (batch 2) |
| [Structures](../theory/structures-fe.md) | Flexural-torsional buckling of frames; a generic pole check without crossarm wind, shear, torsion or foundation; factors on load components | open (batch 2) |
| [Wind on structures](../theory/structure-wind.md) | Snow bridging; allowable loads taken to include the body's ice | open (batch 2) |
| [Wire FE](../theory/wirefe.md) | Structures rigid in the wire solve (sags low for clearances) | open (batch 3) |
| [Spotting](../theory/spotting.md), [sag-tension](../theory/sagtension.md) | Ruling span; holes in the terrain bridged | open (batch 3) |
| [LiDAR](../theory/lidar-analysis.md) | Leaning trees and crowns | open (batch 3) |
| [Thermal](../theory/thermal.md) | Uniform radial temperature; one weather case per section | open (batch 3) |

Already said in the results where they apply (kept): members without a
profile get no ice; wind-band types no ice or snow; a member's centre
line where it has no profile (internal clearances); an offset's moment
left out at a joint of trusses only; the simple check is no design code;
the wire load per unstretched metre (an option).
