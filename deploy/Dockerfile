# syntax=docker/dockerfile:1
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# The Dockerfile itself is permissive, but the produced image bundles the
# AGPL-3.0-only core, so the image label below declares AGPL-3.0-only.

# Enterprise deployment image for the Linxira Knowledge Base (`lkb`).
#
# One kernel, one binary: this image is a single build that serves every
# profile. It defaults to solo/SQLite (`LKB_SQLITE_PATH=/data/lkb.sqlite`) and
# switches to PostgreSQL purely by configuration (`profile`, `storage.backend`,
# `LKB_DATABASE_URL`) — there is no separate "PostgreSQL build".
#
# Build from the repository root:
#   docker build -f deploy/Dockerfile -t ghcr.io/linxira-os/linxira-knowledge-base:dev .
#
# The runtime also carries the built React SPA at /srv/web. The Rust server does
# not serve web/dist itself; mount or copy /srv/web when you want to host the SPA
# (the compose stack instead bind-mounts the checked-out web/dist into nginx).

FROM node:24-bookworm-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

FROM rust:bookworm AS rust-builder
WORKDIR /src
# Exactly the inputs of the Rust build: src/ plus the compile-time embedded
# migrations/ (include_str!) and docs/ seed content (include_str!).
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY docs ./docs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --bin lkb \
    && cp target/release/lkb /usr/local/bin/lkb

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 lkb \
    && useradd --uid 10001 --gid lkb --create-home --home-dir /home/lkb --shell /usr/sbin/nologin lkb \
    && install -d -o lkb -g lkb /data /srv/web

COPY --from=rust-builder /usr/local/bin/lkb /usr/local/bin/lkb
COPY --chown=10001:10001 --from=web /src/web/dist /srv/web

LABEL org.opencontainers.image.title="Linxira Knowledge Base (lkb)" \
      org.opencontainers.image.description="One kernel, three profiles: solo SQLite through enterprise PostgreSQL" \
      org.opencontainers.image.source="https://github.com/Linxira-OS/linxira-knowledge-base" \
      org.opencontainers.image.url="https://github.com/Linxira-OS/linxira-knowledge-base" \
      org.opencontainers.image.licenses="AGPL-3.0-only"

ENV HOME=/home/lkb \
    LKB_SQLITE_PATH=/data/lkb.sqlite \
    LKB_LOG_FORMAT=text

WORKDIR /data
VOLUME ["/data"]
EXPOSE 7331
USER 10001:10001

# `lkb serve` reads ./lkb.toml by default; pass `--config /etc/lkb/lkb.toml`
# (compose does) or any LKB_* environment override instead.
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
  CMD curl -fsS "http://127.0.0.1:${LKB_HEALTHCHECK_PORT:-7331}/health" || exit 1

ENTRYPOINT ["/usr/local/bin/lkb"]
CMD ["serve"]
