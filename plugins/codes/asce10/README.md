# asce10 — ASCE 10-97 for structures (bundled, the default)

The member, joint and bolt-distance checks of lattice structures by
**ASCE 10-97** (*Design of Latticed Steel Transmission Structures*,
American Society of Civil Engineers, 1998), OpenOHL's default member
check, as OpenOHL's core had them until 2026-11-18: moved out of the
core into this plugin with the same formulas and words (owner,
2026-11-17; the second round after SIA 263:2013). **Bundled with
OpenOHL**: the applications and `ohl` find it in their own
`plugins/codes/asce10` before any plugin of the user's, and say at start
when it does not load — never another code in its place.

A structure chooses it as before: no `[analysis] capacity` at all (the
default), or `capacity = "asce10-97"` (the old name is an alias of this
plugin's method `asce10-97`; no file is rewritten). Theory:
`docs/theory/asce10.md`.

| Export | What |
| --- | --- |
| `member-check` | its method `asce10-97` (`check-members` refused: the check needs the full records) |
| `member-check-full` | every checked member in one call, with everything the core's check read; a member without bolts or holes in its data: the net section with a bolt hole in each leg of an angle (the member's typical bolt's, else a 20 mm bolt's standard hole: the program's default), the connection said not checked; a built-up member's bearing on one ply until its end bolts onto a plate of known thickness, said (2026-11-24) |
| `member-check-bracing` | the same with what braces each member: buckling lengths worked out by Figure 3.2, Appendix B's Example 7 and C3.7.4 where the member gives none (2026-11-19; `src/lengths.rs`, `src/bracing.rs`) |
| `joint-check`, `joint-check-full`, `joint-check-moments` | a member's bolts in a gusset plate or cover (bearing, block shear, net section, end and edge distances); a weld between two plates is not checked (ASCE 10-97 gives no weld resistance), said |
| `joint-check-groups` | a joint's bolt groups under the members' forces together (2026-11-19): each bolt's shear (4.3.2), bearing on the plate or member (4.4), end distance along its force (4.5.1), spacing (4.5.2) and edge distance (4.5.3) |
| `joint-check-groups-tension` | the same with the bolts in tension (2026-11-19): the prying of the flanges each bolt bends (the T-stub model of Struik and de Back, α = δ = 1: on the safe side), the bolt's tension (4.3.3, F_t = 0.6 F_u on 0.6 of the gross area), shear and tension together (4.3.4, Eq. 4.3-2), each flange's bending (the model's mechanism) |
| `joint-check-groups-prying` | the same with the gusset plates each bolt bends too (2026-11-19, the owner's decision of that day): a plate is a flange without a web — no moment where the pull comes in, Q = T b′/a′ (on the safe side) — and its bending at the bolt line, T b′ ≤ δ w t² F_y / 4 |
| `joint-check-bends` | a bent plate's bend (2026-11-19, the owner's decision of that day; `src/bends.rs`): the section along the bend line under the force across it, the shear along it and the moments about it and in the plate's plane that OpenOHL works out — Eq. 3.13-1 (P/P_a + M/M_a, P_a = F_y A by 3.10.1, M_a = F_y S of the gross elastic section by 3.14.1–3.14.3), the shear with it by von Mises' criterion √(σ² + 3τ²) ≤ F_y (the standard gives no rule: on the safe side, the peak normal stress and the shear at one point); the standard gives no least bend radius (said) |
| `joint-check-welds` | a weld with its sides (2026-11-25, roadmap O44 J7b): not checked for one fillet or two (ASCE 10-97 gives no weld resistance, 5.2.5), said |
| `bolt-distances`, `bolt-distances-full` | 4.5 with the bolt's force; without an analysis the most one bolt can pass into the part: the smaller of its own design shear capacity (4.3.2: a given capacity, else 0.62 f_u on 0.6 of the gross area, times the shear planes; worked out by the plugin since 2026-11-18, third round) and its bearing cap 1.5 f_u d t (4.4, f_u the lower of the part's and the bolt's; 2026-11-24, O44 L16's question 4), the basis said |

## Parameters

None: ASCE 10-97's design strengths carry no resistance factor to set,
and the core had no setting for its check. A parameter file
`Plugins/asce10.toml` beside a structure must be empty; one with a value
in it is refused by name (never silently left out).

## Time a call may take

As a bundled plugin it is given 120 s a call (user plugins 5 s): one
call checks a whole structure with every load case
(`docs/development/plugins.md`, the measurement).

## Build and test

```bash
cargo build --release --target wasm32-wasip2
cargo test        # natively: the code's own tests, moved with it
```

The proof that the move kept the numbers: `crates/ohl-member-check/tests/sia263.rs`
(`the_examples_keep_the_cores_asce10_numbers`: the shipped examples'
2 256 member results against those the core gave, their hash), and the
core's own tests moved to `crates/ohl-member-check/tests/member_ends.rs`
and `asce10.rs`.

Licence: AGPL-3.0-or-later (OpenOHL's own code, and it uses OpenOHL's
section library).
