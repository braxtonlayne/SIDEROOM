# sr-transcoder: static musl binary plus FFmpeg with NVENC (FND-CI-007, doc 12 section 3).
# Published as ghcr.io/<owner>/sideroom-transcoder:<version>-cuda. Falls back to x264 when
# no GPU is visible. Run with `--gpus all` (NVIDIA Container Toolkit).
#   docker build -f deploy/docker/transcoder.Dockerfile -t sideroom-transcoder .
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

FROM nvidia/cuda:13.4.2-base-ubuntu24.04
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home sideroom
ENV NVIDIA_DRIVER_CAPABILITIES=compute,video,utility
COPY --from=build /out/sr-transcoder /usr/local/bin/sr-transcoder
USER sideroom
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/sr-transcoder"]
