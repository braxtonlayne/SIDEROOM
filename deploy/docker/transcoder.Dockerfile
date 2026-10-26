# sr-transcoder CUDA image: static musl binary plus FFmpeg with NVENC (CIN-ING-006, FND-CI-007,
# doc 12 section 4). Published as ghcr.io/<owner>/sideroom-transcoder:<version>-cuda. Falls back
# to libx264 on the CPU when no GPU is visible. Run with `--gpus all` (NVIDIA Container Toolkit).
#   docker build -f deploy/docker/transcoder.Dockerfile -t sideroom-transcoder:cuda .
#
# FFmpeg comes from the distribution package: it provides h264_nvenc (through the ffnvcodec
# headers, loaded at run time from the host driver), libx264, zscale (libzimg) and the tonemap
# filter. Licence note (ADR-0005): libx264 is GPL, so this image's FFmpeg is GPL; the image is
# distributed with the SIDE ROOM source offer. The check below makes the build fail loudly if a
# future base image drops a codec or filter the pipeline needs.
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

FROM nvidia/cuda:13.4.3-base-ubuntu24.04
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /work sideroom \
    && ffmpeg -hide_banner -encoders 2>/dev/null | grep -q libx264 \
    && ffmpeg -hide_banner -encoders 2>/dev/null | grep -q h264_nvenc \
    && ffmpeg -hide_banner -filters 2>/dev/null | grep -q zscale \
    && ffmpeg -hide_banner -filters 2>/dev/null | grep -q ' tonemap '
ENV NVIDIA_DRIVER_CAPABILITIES=compute,video,utility \
    SIDEROOM_WORKER_TOKEN=
COPY --from=build /out/sr-transcoder /usr/local/bin/sr-transcoder
WORKDIR /work
USER sideroom
VOLUME ["/work"]
STOPSIGNAL SIGTERM
# `docker run ... sideroom-transcoder --check` prints the detected encoders and exits.
ENTRYPOINT ["/usr/local/bin/sr-transcoder", "--work-dir", "/work/jobs"]
