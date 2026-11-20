#!/bin/sh
# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Run once by the PostgreSQL image when its data folder is new: the
# network's database, owned by the server's own role `ohl` (no superuser,
# no new databases or roles), with PostGIS made by the superuser. The
# password comes from the secret file.
set -eu
psql -v ON_ERROR_STOP=1 -v pw="$(cat "$OHL_DB_PASSWORD_FILE")" \
    --username "$POSTGRES_USER" --dbname postgres <<'SQL'
CREATE ROLE ohl LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD :'pw';
CREATE DATABASE network OWNER ohl;
SQL
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname network \
    -c "CREATE EXTENSION IF NOT EXISTS postgis"
