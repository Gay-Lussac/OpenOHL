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
| `member-check-full` | every checked member in one call, with everything the core's check read |
| `joint-check` | a member's bolts in a gusset plate or cover; a weld between two plates |
| `bolt-distances` | Table 17 by bolt size |

## Parameters

`Plugins/sia263.toml` beside a structure file (the design-code plugins'
mechanism: a parameter file per method; the template is the plugin's
`parameters-template`). Every key may be left out: the standard's value.

| Key | Meaning | The standard's |
| --- | --- | --- |
| `resistance.gamma_m1` | γ_M1, members (4.1.3) | 1.05 |
| `resistance.gamma_m2` | γ_M2, net sections and connections (4.1.3) | 1.25 |
| `welds.filler_ultimate` | f_uE, MPa (6.3.2.2) | 510 |

The joints and Member details take the standard's values (they do not
know the structure's folder): the run's note says so, NOT ON THE SAFE SIDE
where the file's factors are larger.

## Build and test

```bash
cargo build --release --target wasm32-wasip2
cargo test        # natively: the code's own tests, moved with it
```

The proof that the move kept the numbers: `crates/ohl-member-check/tests/sia263.rs`
(the shipped examples' 2 256 member results against those the core gave).

Licence: AGPL-3.0-or-later (OpenOHL's own code, and it uses OpenOHL's
section library).
