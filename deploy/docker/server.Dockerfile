# sr-server: static musl binary on a distroless base (FND-CI-007).
# Build from the repository root:
#   docker build -f deploy/docker/server.Dockerfile -t sideroom-server .
# Builds for the platform buildx targets (amd64 or arm64); no cross toolchain needed.
FROM rust:1-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN set -eux; \
    target="$(uname -m)-unknown-linux-musl"; \
    export "CC_$(echo "$target" | tr - _)=musl-gcc"; \
    rustup target add "$target"; \
    cargo build --release --locked --target "$target" -p sr-server; \
    install -D "target/$target/release/sr-server" /out/sr-server

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /out/sr-server /usr/local/bin/sr-server
# Config: SR_CONFIG (TOML path) plus SR_* env vars (docs/dev/server.md).
EXPOSE 8080
# /internal/metrics binds to 127.0.0.1:9100 inside the container by default.
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/sr-server"]
