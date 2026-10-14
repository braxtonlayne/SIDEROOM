# sr-transcoder CPU image: static musl binary plus FFmpeg with libx264 only (CIN-ING-006).
# Published as ghcr.io/<owner>/sideroom-transcoder:<version>-cpu. For machines without an
# NVIDIA GPU: a home server, a spare VPS, a CI runner. Roughly 10 to 20 times slower than NVENC.
#   docker build -f deploy/docker/transcoder-cpu.Dockerfile -t sideroom-transcoder:cpu .
#
# Licence note (ADR-0005): libx264 is GPL, so this image's FFmpeg is GPL.
FROM rust:1-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN set -eux; \
    target="$(uname -m)-unknown-linux-musl"; \
    export "CC_$(echo "$target" | tr - _)=musl-gcc"; \
    rustup target add "$target"; \
    cargo build --release --locked --target "$target" -p sr-transcoder; \
    install -D "target/$target/release/sr-transcoder" /out/sr-transcoder

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /work sideroom \
    && ffmpeg -hide_banner -encoders 2>/dev/null | grep -q libx264 \
    && ffmpeg -hide_banner -filters 2>/dev/null | grep -q zscale \
    && ffmpeg -hide_banner -filters 2>/dev/null | grep -q ' tonemap '
COPY --from=build /out/sr-transcoder /usr/local/bin/sr-transcoder
WORKDIR /work
USER sideroom
VOLUME ["/work"]
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/sr-transcoder", "--work-dir", "/work/jobs", "--force-cpu"]
