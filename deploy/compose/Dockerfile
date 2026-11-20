# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# The network's server image: the command-line program `ohl` (with the
# network's server) on PostgreSQL 17's image, for its pg_dump and
# pg_restore (the backups). Built from the repository's root:
#   docker build -f deploy/compose/Dockerfile -t ohl-network:local .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p ohl-cli --bin ohl

FROM postgres:17-bookworm
COPY --from=build /src/target/release/ohl /usr/local/bin/ohl
COPY --from=build /src/libraries /usr/local/share/ohl/libraries
RUN useradd --system --create-home --home-dir /var/lib/ohl ohl \
    && mkdir -p /backups && chown ohl /backups /var/lib/ohl
USER ohl
WORKDIR /var/lib/ohl
# QUIC (UDP); the web client, if served, on its own TCP port.
EXPOSE 4433/udp
ENTRYPOINT ["ohl"]
CMD ["network", "serve", "--bind", "0.0.0.0:4433"]
