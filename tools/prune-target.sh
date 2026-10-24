#!/bin/sh
# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.
#
# Keeps a Cargo build folder to the builds in use (owner, 2026-10-24: the
# build folder had grown to 300 GB of old builds). Cargo never removes an
# artifact it no longer needs: every change of the code or of a build's
# flags leaves the old one behind. This removes, in every profile
# (debug, release, the wasm targets'), what no build has read for DAYS days
# (by the files' access time): the compiled crates and their records
# (`deps`, `build`, `.fingerprint`, `examples`) file by file, the
# incremental caches crate by crate. Still above MAX_GB: the same with one
# day; still above: that folder's debug builds removed whole (the next
# build makes them again). Only build output is touched; a build later
# makes again whatever it needs.
#
#   tools/prune-target.sh [TARGET_DIR] [DAYS] [MAX_GB]
#
# Defaults: ./target, 3 days, 60 GB. The start scripts and the checks run
# it after building.

set -u
dir="${1:-target}"
days="${2:-3}"
max_gb="${3:-60}"

[ -d "$dir" ] || exit 0

size_gb() {
    du -sk "$dir" 2>/dev/null | awk '{ printf "%d", $1 / 1048576 }'
}

prune() {
    age="$1"
    # The profiles: target/<profile> and target/<triple>/<profile>.
    for profile in "$dir"/* "$dir"/*/*; do
        [ -d "$profile/.fingerprint" ] || continue
        for sub in deps build .fingerprint examples; do
            [ -d "$profile/$sub" ] || continue
            find "$profile/$sub" -type f -atime +"$age" -delete 2>/dev/null
            find "$profile/$sub" -mindepth 1 -type d -empty -delete 2>/dev/null
        done
        # Incremental caches: a crate's whole folder when none of its files
        # was read within the age (never part of one).
        if [ -d "$profile/incremental" ]; then
            for crate in "$profile/incremental"/*; do
                [ -d "$crate" ] || continue
                if [ -z "$(find "$crate" -type f -atime -"$((age + 1))" -print -quit 2>/dev/null)" ]; then
                    rm -rf "$crate"
                fi
            done
        fi
    done
}

before=$(size_gb)
prune "$days"
if [ "$(size_gb)" -gt "$max_gb" ]; then
    prune 1
fi
if [ "$(size_gb)" -gt "$max_gb" ]; then
    rm -rf "$dir/debug"
fi
after=$(size_gb)
echo "prune-target: $dir ${before} GB -> ${after} GB"
