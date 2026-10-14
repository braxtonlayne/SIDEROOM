# Transcode workers and the "rent a GPU" guide

`sr-transcoder` turns uploaded originals into the HLS ladder the Cinema plays (CIN-ING-005,
spec: [docs/specs/cinema-media.md](../docs/specs/cinema-media.md) section 4). It is a worker:
it only makes **outgoing** HTTPS requests to your SIDE ROOM server, so it can run anywhere with
no inbound ports, no VPN and no database access.

> Status: the Admin, Transcoding pages (token creation, drain, queue view) and the Postgres
> job store belong to the catalog and admin work and do not exist yet; this guide describes
> the intended flow. The Dockerfiles in this folder were written without Docker or FFmpeg on the
> authoring machine, so they have **not been built or run**. The first build should be followed
> by `docker run --rm --gpus all <image> --check` (GPU) or `docker run --rm <image> --check`
> (CPU). See "Verify the machine" below.

## Images

| Image | Dockerfile | FFmpeg | Use |
|---|---|---|---|
| `ghcr.io/<owner>/sideroom-transcoder:<ver>-cuda` | `deploy/docker/transcoder.Dockerfile` | `h264_nvenc` + `libx264` | Any NVIDIA GPU box. Falls back to libx264 if no GPU is visible |
| `ghcr.io/<owner>/sideroom-transcoder:<ver>-cpu` | `deploy/docker/transcoder-cpu.Dockerfile` | `libx264` | No GPU. Slow (10 to 20 times slower than NVENC) but works |

Both bundle a static `sr-transcoder` binary and the distribution's FFmpeg. Each build checks
that FFmpeg has `libx264` (and `h264_nvenc` for CUDA), `zscale` and `tonemap`; HDR sources need
the last two. The FFmpeg builds are GPL because of libx264 (ADR-0005); mention it in release
notes.

```sh
docker build -f deploy/docker/transcoder.Dockerfile     -t sideroom-transcoder:cuda .
docker build -f deploy/docker/transcoder-cpu.Dockerfile -t sideroom-transcoder:cpu  .
```

## Run

1. In **Admin, Transcoding**, create a worker token. It is shown once; the server stores only a
   hash. Put it in a file the worker can read (`chmod 600`).
2. Start the worker:

```sh
# GPU (needs the NVIDIA Container Toolkit)
docker run -d --name sr-transcoder --restart unless-stopped --gpus all \
  -v /srv/sr-work:/work -v /srv/sr-token:/run/token:ro \
  ghcr.io/<owner>/sideroom-transcoder:<ver>-cuda \
  --server https://sideroom.example --token-file /run/token --concurrency 2 --name gpu-box

# CPU only
docker run -d --name sr-transcoder --restart unless-stopped \
  -v /srv/sr-work:/work -v /srv/sr-token:/run/token:ro \
  ghcr.io/<owner>/sideroom-transcoder:<ver>-cpu \
  --server https://sideroom.example --token-file /run/token --concurrency 1
```

Without Docker: install FFmpeg 6 or newer (with libx264, zscale, and h264_nvenc for GPUs), put
`sr-transcoder` on the PATH and run `sr-transcoder --server https://... --token-file ...`.
Prefer `--token-file` or the `SIDEROOM_WORKER_TOKEN` environment variable over `--token`, which
shows up in the process list. `http://` servers are refused except `localhost`.

The same thing as a compose service:

```yaml
services:
  transcoder:
    image: ghcr.io/<owner>/sideroom-transcoder:<ver>-cuda
    restart: unless-stopped
    command: ["--server", "https://sideroom.example", "--token-file", "/run/secrets/worker_token", "--concurrency", "2"]
    volumes: ["transcoder-work:/work"]
    secrets: [worker_token]
    deploy:
      resources:
        reservations:
          devices: [{ driver: nvidia, count: all, capabilities: [gpu, video, compute] }]
volumes: { transcoder-work: {} }
secrets: { worker_token: { file: ./worker_token } }
```

### Verify the machine

`sr-transcoder --check` (add `--force-cpu` to ignore the GPU) prints what it found:

```json
{ "encoder": "h264_nvenc", "ffmpeg_version": "6.1.1", "nvenc_available": true, "tonemap": true, "libplacebo": false, "hwaccels": ["cuda", "vaapi"] }
```

`nvenc_available` is true only after a real one-frame NVENC test encode succeeded, not merely
because the encoder is listed. If NVENC fails during a job the worker retries that job step on
libx264 by itself and says so in the job log.

### What the worker does with your disk

Per job it creates `<work-dir>/<job-id>/`, downloads the original (signed URL), writes the
renditions next to it, uploads them, and deletes the folder, also after failures. Plan for
about **3 times the original's size** free in the work directory, plus the output
(about 4.7 GB per hour of content). Jobs that would produce more than the server's budget
(2 times the original, at least 20 GiB) are refused.

## Rent a GPU (checklist)

Transcoding is bursty: you need a GPU for a few hours when a batch of titles is approved, not
every month. Rough numbers from doc 08: an RTX 4090 at about $0.35 to $0.90 per hour handles a
2-hour 1080p film (three-rung ladder) in about 12 minutes, so a film costs roughly $0.10 to
$0.20 and an episode about $0.02 to $0.04.

1. **Pick a host.** Vast.ai or RunPod (hourly marketplace GPUs, pick one with a recent NVIDIA
   driver and at least 100 GB disk), Hetzner GEX (monthly dedicated GPU), or any home PC with
   a GTX 16xx or newer GPU. NVENC needs an NVIDIA GPU; a Docker template with the NVIDIA
   Container Toolkit is the easiest start.
2. **Check the card.** Consumer GeForce cards allow only a few simultaneous NVENC sessions
   (typically 3 to 8 depending on driver); use `--concurrency 2` unless you know your limit.
   Datacenter cards have no such cap. More concurrency does not help once the encoder is busy.
3. **Create the token** in Admin, Transcoding, and note the worker name you will use.
4. **Start the container** with the commands above (or the compose file). The host needs only
   outgoing HTTPS to your server and to your storage endpoint (Garage or Storage Box front).
5. **Check Admin, Transcoding**: the worker appears within a heartbeat (about 20 seconds) with
   its capabilities (NVENC yes or no). Queue a small title first and watch it complete.
6. **Let it drain, then destroy the instance.** Stop claiming new work by marking the worker
   "drain" in Admin (it finishes current jobs), wait for the queue to empty, then terminate the
   rented machine. Revoke the worker token afterwards.

### Security notes for rented machines

- The worker token can claim jobs and read originals through short-lived signed URLs. Treat the
  host as untrusted after you stop paying for it: revoke the token when you are done and use
  one token per rental.
- Originals are downloaded to the machine's disk for the length of the job and removed
  afterwards, but a rented host's operator could still copy them while the job runs. Do not
  process content you could not hand to the host's operator.
- Uploads use presigned URLs scoped to one job's output paths; the worker holds no storage keys.
- FFmpeg parses hostile media. Run the container as the default non-root user (the images do)
  and do not mount anything except the work directory.

### Troubleshooting

| Symptom | Likely cause |
|---|---|
| `--check` says `encoder: null` | FFmpeg missing or without libx264 |
| `nvenc_available: false` on a GPU box | Container started without `--gpus all`, driver too old for the FFmpeg's NVENC API (log says "Driver does not support the required nvenc API version"), or session limit reached |
| Job fails: "this ffmpeg cannot tone-map HDR" | FFmpeg built without `zscale`/`tonemap`; use the provided images |
| Job fails: "Dolby Vision profile 5 needs an ffmpeg built with libplacebo" | Profile 5 files have no HDR10/SDR fallback layer; use a libplacebo build with Vulkan, or re-source the file |
| Job requeued repeatedly, `lease expired` | Worker lost its connection for more than 2 minutes; check the network |
| Jobs fail with "the file could not be decoded" | Corrupt upload; the failure is not retried. Re-upload |
