# ADR-0012: Plugin interface versions and compatibility

- **Status:** Proposed (built 2026-11-07, in active development; the owner
  has not approved it yet)
- **Date:** 2026-11-07
- **Amends:** [ADR-0003](0003-plugin-mechanism.md) (its "semantic versioning
  and compatibility tests required")

## Context
ADR-0003 made the WebAssembly interface a public contract. Until now that
contract was kept by care: the [plugin guide](../development/plugins.md)
says of each addition that "the interface version stays 0.3" and that
plugins built before load unchanged, but nothing tested it. A change that
broke plugins already built would have been found by their authors, after
a release.

Two numbers name the interface. Manifests say `sdk = "0.3"`, the version
the host checks. The WIT package is `ohl:plugin@0.1.0`, the name every
import and export of a built plugin carries. They differ because the
package was renamed (2026-10-18) without a breaking change of its content.

Writing a plugin outside this repository was awkward: every plugin pointed
`wit-bindgen` at `crates/ohl-plugins/wit` by a relative path.

## Options considered
1. **Unify the numbers now** (`ohl:plugin@0.4.0`, `sdk = "0.4"`): one number,
   but every plugin built, the owner's own included, must be rebuilt for no
   change of content.
2. **Keep both, document them, change the package version only with a
   breaking change** (chosen by the owner, 2026-11-07).
3. **Test compatibility by reviewing changes**: what was done until now.
4. **Test it mechanically** against frozen copies and plugins kept as built
   (chosen).

## Decision
**One interface version, `sdk = "MAJOR.MINOR"`** (`ohl_plugins::SDK_VERSION`,
now 0.3), is what a plugin targets and what the host checks. The WIT
package's version (`ohl:plugin@0.1.0`) is part of the names in a built
plugin; it changes only with a breaking change, and is then made to match.

**Additive changes keep the version.** A new interface, a new world, a new
optional export of any plugin (the host looks for it by name), a new
extension point, a new optional manifest key: a plugin built before loads
and answers as it did.

**Breaking changes make a new version.** Changing or removing a type,
function or world export of a released version; a new export required of an
existing world; a manifest key made required. Before 1.0 a breaking change
raises the minor version (0.3 → 0.4) and the host loads only the current
one, as today. From 1.0 it raises the major version, and the host keeps
loading plugins of the previous major version beside the new one for a
time the owner sets before 1.0 (proposed: at least 12 months).

**The guard** (`tools/check.sh` and the tests):
- `crates/ohl-plugins/wit-frozen/<version>/`: a copy of each released
  interface, never edited. A test compares the live interface with each:
  every interface keeps every type and function exactly; every world keeps
  its imports and exactly its exports.
- `crates/ohl-plugins/tests/compat-<version>/`: one plugin of each world,
  kept as built (`tools/plugin_fixtures.sh`, local paths mapped out of the
  binaries). A test loads each and runs `ohl plugin check` on it.
- Every plugin of the repository and the owner's private ones are checked
  with `ohl plugin check`, and a plugin of every kind is made from its
  template, built and checked, on every full check.

**The tools of the SDK**, in the command line (and so independent of this
repository): `ohl plugin new NAME --kind KIND` writes a plugin's folder that
builds on its own (its manifest, Cargo.toml, the WIT files, a skeleton);
`ohl plugin wit DIR` writes the WIT files the program is built with;
`ohl plugin check DIR…` checks a plugin as the applications will use it;
`ohl plugin points` lists the extension points and their worlds.

## Consequences
- A plugin author builds outside the repository from the WIT files the
  program carries, and checks the result before sharing it.
- A change of the host or the interface that breaks a plugin already built
  fails the tests before it is pushed, not in the authors' hands.
- Adding to a released world's exports is caught as breaking: additions go
  into new interfaces or worlds, as was already the practice
  (`code-wind-plugin`, `code-rules-plugin`, the `-export` worlds).
- The fixtures are binaries in the repository (about 0.8 MB). They are
  rebuilt only on purpose, never to make a failing test pass.
- Still open (GitHub issue #34): a plugin registry or index, and the time
  the host keeps loading an older major version from 1.0.
