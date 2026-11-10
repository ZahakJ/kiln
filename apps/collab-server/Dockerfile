# syntax=docker/dockerfile:1
# The Kiln collaboration relay. Build from the repository root (the relay is built from the
# workspace):
#   docker build -f apps/collab-server/Dockerfile -t kiln-collab-server .
# or, from this directory, `docker compose build`.

FROM rust:1.99.1-slim-trixie AS build
WORKDIR /src
# The workspace manifests and the crates; the toolchain file and the local Cargo settings
# (Android targets, sccache) are left out: the image's toolchain is the pinned one.
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY xtask xtask
COPY apps/desktop/src-tauri apps/desktop/src-tauri
# The bundled Deck themes the presentation model embeds.
COPY assets/themes assets/themes
ARG JOBS=6
RUN cargo build --release --locked --jobs "${JOBS}" -p kiln-collab-server

FROM debian:trixie-slim
RUN useradd --system --uid 10001 --home-dir /data --no-create-home kiln \
    && mkdir /data && chown kiln /data
COPY --from=build /src/target/release/kiln-collab-server /usr/local/bin/kiln-collab-server
USER kiln
VOLUME /data
EXPOSE 7070
# The token comes from KILN_COLLAB_TOKEN.
ENTRYPOINT ["kiln-collab-server", "--bind", "0.0.0.0:7070", "--data", "/data"]
