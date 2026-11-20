#!/bin/sh
# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# The backup job: `ohl network backup` every OHL_BACKUP_EVERY_HOURS hours
# into /backups/ohl-YYYYMMDD-HHMMSS (beside the running server: one
# snapshot, nothing waits), the newest OHL_BACKUP_KEEP kept. Each backup
# is recorded in the network (a schema migration asks for a recent one).
# Copy /backups elsewhere too (another host, another site).
set -u
EVERY=${OHL_BACKUP_EVERY_HOURS:-6}
KEEP=${OHL_BACKUP_KEEP:-28}
USER_NAME=${OHL_BACKUP_USER:?the network admin the backups are made by}
while true; do
    OUT="/backups/ohl-$(date -u +%Y%m%d-%H%M%S)"
    if ohl network backup --out "$OUT" --user "$USER_NAME"; then
        # The oldest beyond KEEP removed (only complete backups count).
        ls -1d /backups/ohl-* 2>/dev/null | sort | head -n "-$KEEP" | while read -r old; do
            rm -rf "$old"
        done
    else
        echo "backup failed: $OUT" >&2
        rm -rf "$OUT"
    fi
    sleep $((EVERY * 3600))
done
