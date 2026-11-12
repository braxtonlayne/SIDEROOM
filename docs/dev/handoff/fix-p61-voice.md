# Wave 61 handoff: voice (SEC-AUD-142..143, D-631)

Branch `fix/p61-voice`. Files: `crates/sr-voice/**` only. No migration, no route or response change, no UI.

## Done
- `crates/sr-voice/src/ingress.rs` (new): byte token bucket per (participant, source), the shape of `datachannel::Budget`: rate, one second of burst (never less than a floor: 2 KiB audio, 512 KiB video), refill by time. Drop is not queued; one `warn` per participant and source per minute; a layer that lost a frame reports `Resumed` on its next forwarded frame.
- `core.rs`: `forward_audio` takes the frame size from the bucket after the `allowed` check and before the speaking detector and forwarding (SEC-AUD-142); the music cap is recomputed from `music_of(channel)` on every frame, so `set_music` needs no change. `forward_video` takes it before layer stats and forwarding, one bucket over all layers of the source (SEC-AUD-143); on `Resumed` (and the frame is not itself a keyframe) it calls `request_keyframe` (its own coalescing and rate limit apply). New stats `ingress_dropped_audio` / `ingress_dropped_video` in `Snapshot`, merged in `server.rs`. Builder `SfuCore::with_video_caps(screen_kbps, camera_kbps, burst_floor)` for tests. `SfuConfig` untouched.
- Not done: Prometheus counters for the two new stats (they are in `Snapshot` only; `metrics.rs` `Deltas` not extended).

## Highest legitimate client rates and the caps
| Track | Highest our client sends | Where | Cap |
|---|---|---|---|
| mic, screen audio | 384 kbit/s (`BITRATE_MAX`) | `controller.ts:1359` `bitrateOf`, `engine.ts:738-747` | 640 kbit/s (x1.5 + 64) |
| music | room `music_bitrate_kbps`, 32..=320 | `engine.ts:747`, `core.rs` `clamp_music_kbps` | room kbps x 1.5 + 64 (544 at 320) |
| screen video | 7 Mbit/s: 5000 x 1.4 above 720 lines and 30 fps | `engine.ts:1155` `screenBitrate` (presets up to 1080p60, `video.ts:49-53`) | 14 Mbit/s |
| camera video | 8.12 Mbit/s: 1080p60 simulcast, f 3500 x 1.4 = 4900, h (540 lines) 1700 x 1.4 = 2380, q (270 lines) 600 x 1.4 = 840 kbit/s | `video.ts:102-120` `simulcastEncodings` | 16.24 Mbit/s (2x the sum) |
The camera number is mine (the task left it to be read from `engine.ts`/`video.ts`). Payload only: headers, E2EE overhead and retransmissions are the headroom. The load test (2.15 Mbit/s per camera, 32 kbit/s audio) stays well under; no change needed there.

## Tests
Run: `cargo clippy -j 4 -p sr-voice --all-targets -- -D warnings` (clean); `--lib ingress` (8 pass); `--test music --test video --test load_smoke` (music 10, video 11, load_smoke 2: all pass; load_smoke was run before the final test-only edits).
New: unit `a_track_within_its_cap_loses_nothing`, `a_track_over_its_cap_is_dropped_to_the_cap`, `the_bucket_has_a_one_second_burst_and_refills`, `a_keyframe_always_fits_a_full_bucket`, `a_screen_share_at_the_clients_maximum_loses_nothing`, `simulcast_layers_share_one_bucket`, `the_warning_is_rate_limited`, `set_kbps_follows_the_music_bitrate`; real str0m clients: `music_over_the_rooms_bitrate_is_dropped_at_the_sfu` (room 64 kbit/s, sender 320), `a_microphone_at_the_highest_channel_bitrate_is_untouched` (384 kbit/s mic and 320 music for 10 s, 0 drops), video `a_video_source_over_its_cap_is_dropped_and_a_keyframe_is_requested_when_it_resumes`, `simulcast_layers_share_one_cap`.
Honest note: I did not run the new integration tests against the old code. They read `Snapshot::ingress_dropped_*`, which does not exist without the change, so they cannot pass without it. The video tests use `with_video_caps` with small caps (a loopback cannot push 14 Mbit/s); the real 14/16.24 Mbit/s constants are covered by the unit tests only. A real screen-source cap in the loopback was not tested (camera path uses the same code).
Finding while testing: a flooding publisher alternates drop and resume, so each resume asks for a keyframe; the coalescer limits that to its normal rate.

## Proposed inventory rows
- SEC-AUD-142: Audio ingress cap per (participant, source) in `sr-voice` `forward_audio` (`ingress.rs`): music room kbps x 1.5 + 64, mic and screen audio 640 kbit/s, one second burst; excess dropped and counted (`ingress_dropped_audio`), warn at most once a minute, nobody disconnected. Tests: `music_over_the_rooms_bitrate_is_dropped_at_the_sfu`, `a_microphone_at_the_highest_channel_bitrate_is_untouched`, ingress unit tests.
- SEC-AUD-143: Video ingress cap per (participant, source) over all simulcast layers in `forward_video`: screen 14 Mbit/s, camera 16.24 Mbit/s; excess dropped for all subscribers and counted (`ingress_dropped_video`); keyframe requested when forwarding resumes. Tests: `a_video_source_over_its_cap_is_dropped_and_a_keyframe_is_requested_when_it_resumes`, `simulcast_layers_share_one_cap`.

## Proposed threat-model row (D-631)
Threat: a modified client floods the node egress through one track (any audio or video rate times the viewer count). Mitigation: per-track ingress token buckets in the SFU, fixed above the client maximum, with the tests above. Residual: fixed constants with no operator setting; a single-layer source under its cap still cannot be lowered by the governor; a flooder under the cap but near it still costs up to 14-16 Mbit/s per viewer.

## Needs other crates (flagged, not edited)
- Operator setting for the caps (`voice.ingress_*` via `sr-core` config and `sr-server/src/voice.rs` `SfuConfig`).
- Prometheus counters for the dropped frames (`metrics.rs` is in sr-voice; I left it for a follow-up).

## Owner questions
None blocking. Default chosen: camera cap 2x the 1080p60 simulcast sum (16.24 Mbit/s).
