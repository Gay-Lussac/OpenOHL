# Lap joints, splices and continuous members

Implemented in `crates/ohl-structures-fe/src/member_ends.rs`
(`worked_planes`, `link_continuous`, `continuous_problem`) and
`crates/ohl-structures-fe/src/asce10.rs` / `sia263.rs` (`end_bolts`,
`onto_leg`, each member end's connection check), with the data of
`crates/ohl-structures/src/connections.rs` (`Face`, `TypeSplice`,
`ConnectionKind::Continuous`). Built, in active development (not approved
yet; [proposal](../development/joint-details.md#lap-joints-double-covers-continuous-members-proposal-2026-11-05));
pre-alpha: results must be verified by a qualified engineer.

**Sources.** ASCE 10-97, *Design of Latticed Steel Transmission
Structures* (American Society of Civil Engineers), and SIA 263:2013,
*Construction en acier* (Swiss Society of Engineers and Architects).
Clause, table and equation numbers below are the standards'. Every
assumption is marked **on the safe side** (conservative) or **not on
the safe side**.

## Three ways two members meet

- **Lapped** (`face`): a member end bolted directly onto another member's
  leg — on its outside face (the default), its inside face, or **nested**,
  an angle heel into heel inside the other. The bolts pass through two
  legs: **one shear plane**.
- **Spliced with covers** (`[[splice]]`): the members' legs sandwiched
  between covers. A bolt through a leg and the covers on it has **one
  shear plane per cover**: two covers (outer and inner), two planes.
- **Continuous** (`kind = "continuous"`): one part running on through the
  joint, no bolts.

## Shear planes

The shear planes per bolt are worked out from the plies the bolts pass
through (`EndDesign::planes`): a splice's covers on the leg the end is
bolted by (both legs bolted: the leg with fewer covers, **on the safe
side**), one for an end lapped onto a member or a plate. Which value the
check takes:

1. the bolts' own `shear_planes`, when given (a value differing from the
   worked-out one is said);
2. a splice's worked-out planes;
3. the member's design data (`[member.design] bolts.shear_planes`);
4. a lap's worked-out one plane;
5. one.

The bolt shear is then \\( n \cdot m \cdot R_v \\) with \\( m \\) the
planes (ASCE 10-97 4.3; SIA 263 Table 16, (73)/(74), per plane).

## Bearing on both legs of a lap

Where a member end with a connection is bolted onto another member (given
or worked out: the joint's leg), the bolts bear on **both** legs. The
check takes the lower of the member's own bearing and the bearing on the
leg it bolts onto (its thickness: that member's design data, else its
angle's leg):

| Standard | Bearing on the leg bolted onto | Clause |
|---|---|---|
| ASCE 10-97 | \\( n \cdot 1.5\,F_u\,d\,t_o \\), \\( F_u \\) the lower of that member's and the bolt's | 4.4 |
| SIA 263 | per bolt \\( \min(0.85\,e_1/d_0,\ 2.4)\,f_u\,d\,t_o/\gamma_{M2} \\) | Table 16, (76) |

Under SIA 263 the end distance \\( e_1 \\) in the leg bolted onto is not
known (the bolts sit where the member's own end puts them): the member's
own \\( e_1 \\) is taken, **not on the safe side** where that leg ends
nearer the bolts (said in the results). A member without connection data
is checked as before (its own leg only).

## Continuous members

Two members are one part through a joint when `kind = "continuous"` is
given on one end (the other end not saying otherwise), they leave the
joint in line within **1°** (the owner's tolerance) and they have the
same section, roll and reference (`continuous_problem`). Otherwise the
kind is dropped and the end computes as bolted, as before (said).

At a continuous end:

- **no connection is checked**: no bolt shear, bearing, net section or
  block shear (no holes); the tension there is the gross section;
- the end is **concentric** (worked out, "continuous into L2"): the
  force passes straight on, so the slenderness takes the concentric case
  at that end (ASCE 10-97 3.7.4.2);
- the end is **not taken as restrained** against rotation from the
  continuity: the panel beyond is of the same section and can buckle with
  this one, and ASCE 10-97 C3.7.4 says a joint whose members can all
  buckle at once restrains none of them. The restraint stays as given,
  the member's design data or assumed (none) — **on the safe side**;
- the **buckling length is unchanged**: still from joint to joint, each
  joint taken as a bracing point as before (a joint where no bracing
  arrives is not one; that is stage 4.3 of the joint details, not built —
  **on the safe side** only where the joint does brace);
- the **analysis is unchanged**: members already share their joint's
  node, and a connection's kind (pinned, fixed, continuous) does not
  change the finite-element model's stiffness.

## Covers

Each cover of a splice (`[[splice.cover]]`) lies on its face of the
members' leg, its outline given or worked out round the bolts on that leg
(`ohl_structure_link::joint_details`). Per member end bolting onto the
splice, each cover on the leg it is bolted by is checked **as a gusset
plate is** ([Gusset plates and their welds](joint-plates.md)): the bolts'
bearing on the cover, its block shear round the bolt group, its section
across the group, its end and edge distances
(`plate_checks::cover_plans`, run with every case).

**The force each cover takes.** The member's force is shared between the
covers on that leg **by area**, each cover's thickness times its width
across the member over the covers' together:

\[ N_c = N\,rac{t_c\,b_c}{\sum_k t_k\,b_k} \]

That is the share of covers straining alike (the same steel, the bolts
in their holes): **an assumption**, said in the results. It is **not on
the safe side** where the bolts' slip or the holes' clearance lets one
cover take more before the other bears; the bolt shear itself is checked
in the member's own check with every shear plane. An inner cover starts
clear of the root (the leg's thickness plus its root radius), so it is
narrower than the outer one and takes less: for an L 61 × 7 with 8 mm
covers, 61 and 46 mm wide, 57 % and 43 %.

A cover's steel is its material's; none given, the weakest of the
members bolting onto the splice (assumed, said).
