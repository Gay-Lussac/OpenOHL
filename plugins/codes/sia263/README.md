# sia263 — SIA 263:2013 for structures (bundled)

The member, joint and bolt-distance checks of lattice structures by
**SIA 263:2013** (*Construction en acier*), as OpenOHL's core had them
until 2026-11-18: moved out of the core into this plugin with the same
formulas and words (owner, 2026-11-17). **Bundled with OpenOHL**: the
applications and `ohl` find it in their own `plugins/codes/sia263`
before any plugin of the user's, and say at start when it does not
load — never another code in its place.

A structure chooses it as before: `[analysis] capacity = "sia263-2013"`
(the old name is an alias of this plugin's method `sia263-2013`; no file
is rewritten). Theory: `docs/theory/sia263.md`.

| Export | What |
| --- | --- |
| `member-check` | its method `sia263-2013` (`check-members` refused: the check needs the full records) |
| `member-check-full` | every checked member in one call, with everything the core's check read; a member without bolts or holes in its data: (39) with a bolt hole in each leg of an angle (the member's typical bolt's, else a 20 mm bolt's hole of Table 17: the program's default), the connection said not checked; a built-up member's bearing (76) on one ply until its end bolts onto a plate of known thickness, said (2026-11-24) |
| `member-check-bracing` | the same with what braces each member: buckling lengths between the joints held in the direction of buckling (5.4.1.3, read strictly), lateral supports of at least 1 % (4.2.3.7), where the member gives none (2026-11-19; `src/lengths.rs`, `src/bracing.rs` — the same file as `asce10`'s) |
| `member-check-default-bolts` | the same with OpenOHL's default bolt for each member without bolts in its data (2026-11-26, O19 stage 5): its hole in each leg after the member's own bolts at its other end, before the plugin's 20 mm bolt — the largest bolt of the structure's library the leg's usual gauge takes (mid-leg, the hole clear of the root fillet, Table 17's minimum edge distance e_2 kept to the toe), else the structure's most used bolt; its hole given, else Table 17's; a size Table 17 does not list not taken; where it comes from said with the member, not on the safe side (a larger bolt or more holes lower the net section) |
| `joint-check`, `joint-check-full`, `joint-check-moments` | a member's bolts in a gusset plate or cover; a weld between two plates (with the parameter file: `joint-check-full`; with the moments of a weld's force and a frame member's end shear and moments: `joint-check-moments`) |
| `joint-check-groups` | a joint's bolt groups under the members' forces together (2026-11-19): each bolt's shear (Table 16, (73)–(75)), bearing (76) with e₁ along its force, Table 17's distances along and square to it |
| `joint-check-groups-tension` | the same with the bolts in tension (2026-11-19): the prying of the flanges each bolt bends (6.2.2.4 gives no method: the T-stub model of Struik and de Back, α = δ = 1, on the safe side), the bolt's tension (Table 16, (77), A_s 0.6 of the gross area), shear and tension together (6.2.2.6, (78)), each flange's bending over γ_M1 |
| `joint-check-groups-prying` | the same with the gusset plates each bolt bends too (2026-11-19, the owner's decision of that day): a plate is a flange without a web — no moment where the pull comes in, Q = T b′/a′ (on the safe side) — and its bending at the bolt line, T b′ ≤ δ w t² f_y / (4 γ_M1) |
| `joint-check-bends` | a bent plate's bend (2026-11-19, the owner's decision of that day; `src/bends.rs`): the section along the bend line under the force across it, the shear along it and the moments about it and in the plate's plane that OpenOHL works out — the comparison stress of 4.3.5.4, formula (6), √(σ² + 3τ²) ≤ f_y/γ_M1, the stresses elastic (5.2.1, 5.2.6: W_el, on the safe side — no plastic resistance relied on in a cold-bent zone); the inside radius against 5.7.1.2, Table 14 (the least radius where a weld is made within 5 t of the bend: NOT ON THE SAFE SIDE where the bent plate has a weld and the radius is smaller, a note without one) |
| `joint-check-welds` | a weld with its sides (2026-11-25, roadmap O44 J7b; `src/joints.rs`, `weld_sides`): (87) and (88) on the force per unit length at the most loaded end of each fillet — one fillet (sides 1): its throat (contact width) bent across by the moment about the seam, elastic (its root opened: an engineering model, not the standard's rule, on the safe side; 6.3.6.5 asks to avoid such a fillet, said); two fillets (sides 2, each half the length given, or the seam's): the moment about the seam a couple across the thinner plate's thickness (on the safe side), the moment along them on both lines; sides not known: both, the worse governing |
| `joint-check-groups-packing` | the joint's bolt groups with the packings in each bolt's grip (2026-11-27, roadmap O49 stage 1c; `src/groups.rs`, `shear_reductions`): 6.2.2.5, β_p = 9 d/(8 d + 3 t_p) ≤ 1 for packings above d/3; in every group export also 6.2.2.2, β_Lf for a long joint (L_j per bolt from the group's bolts carrying the same parts) — and in the member check from the end's bolt pattern |
| `bolt-distances`, `bolt-distances-full` | Table 17 by bolt size; the force per bolt shown with them without an analysis its own design shear resistance by Table 16 (73)/(74) over γ_M2 (third round, 2026-11-18; Table 17 takes no force) |

## Parameters

`Plugins/sia263.toml` beside a structure file (the design-code plugins'
mechanism: a parameter file per method; the template is the plugin's
`parameters-template`). Every key may be left out: the standard's value.

| Key | Meaning | The standard's |
| --- | --- | --- |
| `resistance.gamma_m1` | γ_M1, members (4.1.3) | 1.05 |
| `resistance.gamma_m2` | γ_M2, net sections and connections (4.1.3) | 1.25 |
| `welds.filler_ultimate` | f_uE, MPa (6.3.2.2) | 510 |

The member check, the joints' plates and welds (`joint-check-moments`, `joint-check-full`) and
Member details all take the file's values (since 2026-11-18: before, the
joints and Member details took the standard's); the run's note names the
file. The bolt tools' minimum distances (Table 17) do not depend on them; the force per bolt shown with them (Table 16 over γ_M2) does.

## Build and test

```bash
cargo build --release --target wasm32-wasip2
cargo test        # natively: the code's own tests, moved with it
```

The proof that the move kept the numbers: `crates/ohl-member-check/tests/sia263.rs`
(the shipped examples' 2 256 member results against those the core gave).

Licence: AGPL-3.0-or-later (OpenOHL's own code, and it uses OpenOHL's
section library).
