# sr-server: static musl binary on a distroless base (FND-CI-007).
# Build from the repository root:
#   docker buildx build -f deploy/docker/server.Dockerfile --platform linux/amd64,linux/arm64 -t sideroom-server .
# The build stage runs on the build host's native architecture and cross-compiles
# to the target architecture with cargo-zigbuild (zig as the C cross compiler), so
# arm64 images are not built under QEMU emulation (Q109).
FROM --platform=$BUILDPLATFORM rust:1-bookworm AS build
ARG TARGETARCH
RUN apt-get update && apt-get install -y --no-install-recommends python3-pip \
    && pip3 install --break-system-packages --no-cache-dir ziglang==0.13.0 \
    && cargo install cargo-zigbuild --locked \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN set -eux; \
    case "$TARGETARCH" in \
        amd64) target=x86_64-unknown-linux-musl ;; \
        arm64) target=aarch64-unknown-linux-musl ;; \
        *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    rustup target add "$target"; \
    cargo zigbuild --release --locked --target "$target" -p sr-server; \
    install -D "target/$target/release/sr-server" /out/sr-server

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /out/sr-server /usr/local/bin/sr-server
# Config: SR_CONFIG (TOML path) plus SR_* env vars (docs/dev/server.md).
EXPOSE 8080
# /internal/metrics binds to 127.0.0.1:9100 inside the container by default.
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/sr-server"]
