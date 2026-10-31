#!/usr/bin/env python3
# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.
"""Keep only the newest builds of every crate in a Cargo build profile.

    python3 -I tools/prune_builds.py PROFILE_DIR [KEEP] [DAYS]

PROFILE_DIR: e.g. target/debug. KEEP: builds kept per crate and kind
(default 2; owner, 2026-10-29: "only keep the last 2 builds at all
times"). DAYS: also every build none of whose files was read for DAYS
days, removed whole (2026-10-31: removed file by file, a library kept its
`.rmeta` and lost its `.rlib`, its record stayed, and the doc tests failed,
"can't find crate"). Cargo names every build of a crate by a hash of its source, flags
and features (`deps/<crate>-<hash>.*`) and never removes one it no longer
needs: each change of the code leaves the old build behind — its
libraries, its test programs and, on macOS with debug information, all its
object files. This keeps, per crate and kind (library, program or test,
dynamic library), the KEEP builds written last and removes every file of
the others, with their record in `.fingerprint` (2026-10-31: without it
Cargo took a removed library for up to date and a later build failed,
"extern location … does not exist"); in `incremental`, the KEEP newest
caches per crate. Nothing else is touched (not `build`): a build later
makes again whatever it needs.
"""

import os
import re
import shutil
import sys
from collections import defaultdict

NAME = re.compile(r"^(?:lib)?(.+?)-([0-9a-f]{16})(\..*)?$")


def kind_of(files):
    exts = {f[2] for f in files}
    if any(e and (e.endswith(".rlib") or e.endswith(".rmeta")) for e in exts):
        return "lib"
    if any(e and (e.endswith(".dylib") or e.endswith(".so") or e.endswith(".dll")) for e in exts):
        return "dylib"
    if any(e is None for e in exts):
        return "bin"
    return "other"


def prune_deps(deps, keep, gone, days=None):
    builds = defaultdict(list)  # (crate, hash) -> [(path, mtime, ext)]
    for f in os.listdir(deps):
        m = NAME.match(f)
        p = os.path.join(deps, f)
        if not m or not os.path.isfile(p):
            continue
        builds[(m.group(1), m.group(2))].append((p, os.path.getmtime(p), m.group(3)))
    by_crate = defaultdict(list)  # (crate, kind) -> [(newest mtime, hash)]
    for (crate, h), files in builds.items():
        by_crate[(crate, kind_of(files))].append((max(f[1] for f in files), h))
    removed = 0
    if days is not None:
        import time
        old = time.time() - days * 86400
        for (crate, h), files in builds.items():
            read = max(max(os.path.getatime(f[0]), f[1]) for f in files)
            if read < old:
                gone.add(f"{crate}-{h}")
                for p, _, _ in files:
                    try:
                        removed += os.path.getsize(p)
                        os.remove(p)
                    except OSError:
                        pass
    for (crate, _), hashes in by_crate.items():
        hashes.sort(reverse=True)
        for _, h in hashes[keep:]:
            if f"{crate}-{h}" in gone:
                continue
            gone.add(f"{crate}-{h}")
            for p, _, _ in builds[(crate, h)]:
                try:
                    removed += os.path.getsize(p)
                    os.remove(p)
                except OSError:
                    pass
    return removed


def prune_incremental(inc, keep):
    caches = defaultdict(list)
    for d in os.listdir(inc):
        p = os.path.join(inc, d)
        m = NAME.match(d)
        if m and os.path.isdir(p):
            caches[m.group(1)].append((os.path.getmtime(p), p))
    removed = 0
    for lst in caches.values():
        lst.sort(reverse=True)
        for _, p in lst[keep:]:
            for root, _, files in os.walk(p):
                for f in files:
                    try:
                        removed += os.path.getsize(os.path.join(root, f))
                    except OSError:
                        pass
            shutil.rmtree(p, ignore_errors=True)
    return removed


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    profile = argv[0]
    keep = int(argv[1]) if len(argv) > 1 else 2
    days = float(argv[2]) if len(argv) > 2 else None
    removed = 0
    gone = set()
    for sub in ("deps", "examples"):
        d = os.path.join(profile, sub)
        if os.path.isdir(d):
            removed += prune_deps(d, keep, gone, days)
    # Their records, so that Cargo builds them again when it needs them —
    # and any record left by an earlier prune whose library is gone (a record names the package with hyphens, a build the crate
    # with underscores).
    fp = os.path.join(profile, ".fingerprint")
    deps = os.path.join(profile, "deps")
    present = set()
    if os.path.isdir(deps):
        for f in os.listdir(deps):
            m = NAME.match(f)
            # A library is there with its `.rlib` or `.rmeta` both or
            # alone as built; one left with only part of its files is not.
            if m:
                present.add(f"{m.group(1)}-{m.group(2)}")
    if os.path.isdir(fp):
        for d in os.listdir(fp):
            m = NAME.match(d)
            if not m:
                continue
            key = f"{m.group(1).replace('-', '_')}-{m.group(2)}"
            p = os.path.join(fp, d)
            # Libraries only: a dependency's library that is gone is not
            # built again (a program or test that is gone is).
            built = any(f.startswith("lib-") for f in os.listdir(p))
            if key in gone or (built and key not in present):
                shutil.rmtree(p, ignore_errors=True)
    inc = os.path.join(profile, "incremental")
    if os.path.isdir(inc):
        removed += prune_incremental(inc, keep)
    print(f"prune-builds: {profile}: {removed / 1e9:.1f} GB of older builds removed (the last {keep} kept)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
