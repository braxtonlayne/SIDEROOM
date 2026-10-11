# ADR-0005: FFmpeg (via Rust bindings) and native image/archive codecs

- **Status:** accepted
- **Date:** 2026-10-11

## Context
We transcode arbitrary input video (H.264/HEVC/AV1/VP9/MPEG-2, many containers, subtitle formats) into an H.264 HLS ladder, using NVENC GPUs. We also convert manga images to WebP and read PDFs and RAR archives.

## Decision
- Video: **FFmpeg libraries via `ffmpeg-next`** for demux/decode/scale/encode (NVENC `h264_nvenc`, CPU `libx264`) and HLS muxing. Orchestration, job control, probing logic, and manifest generation are Rust.
- AV1 (optional later rung): `rav1e` (Rust) for CPU, or `av1_nvenc` on Ada+ GPUs.
- Images: `image` + `fast_image_resize` (Rust). Lossy WebP encoding via `webp` (libwebp bindings), because pure-Rust WebP encoders are lossless-only.
- PDF rasterization: `pdfium-render`. RAR: `unrar` bindings. ZIP/7z: pure Rust (`zip`, `sevenz-rust`).

## Consequences
The transcoder image bundles FFmpeg with CUDA. Licensing: build FFmpeg LGPL-only, except libx264 (GPL) in the worker image. Document this in the release notes.
