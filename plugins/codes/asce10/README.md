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
| `member-check-full` | every checked member in one call, with everything the core's check read |
| `joint-check`, `joint-check-full` | a member's bolts in a gusset plate or cover (bearing, block shear, net section, end and edge distances); a weld between two plates is not checked (ASCE 10-97 gives no weld resistance), said |
| `bolt-distances`, `bolt-distances-full` | 4.5 with the bolt's force; without an analysis its own design shear capacity (4.3.2: a given capacity, else 0.62 f_u on 0.6 of the gross area, times the shear planes), worked out by the plugin since 2026-11-18 (third round; before, by the core — the same numbers) |

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
