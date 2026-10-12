# sr-voice: static musl binary on a distroless base (FND-CI-007).
# Build from the repository root:
#   docker build -f deploy/docker/voice.Dockerfile -t sideroom-voice .
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
    cargo build --release --locked --target "$target" -p sr-voice; \
    install -D "target/$target/release/sr-voice" /out/sr-voice

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /out/sr-voice /usr/local/bin/sr-voice
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/sr-voice"]
