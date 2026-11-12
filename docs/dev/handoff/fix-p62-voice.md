# Wave 62 handoff: voice (SEC-AUD-152..153, D-639)

Branch `fix/p62-voice`. Files: `crates/sr-voice/**` and one line in `docs/12-deployment-and-operations.md` (section 7). No migration, no route or response change, no UI, no operator setting.

## Done
- SEC-AUD-152 (D-639): `sr_voice_ingress_dropped_frames_total{kind="audio"|"video"}` in `metrics.rs` (described at startup). New `metrics::Counts` (`of(&Snapshot)`, `since(&last) -> Deltas`) replaces the 4-tuple in `server.rs`, so the delta arithmetic is one tested function; `Deltas` gained `ingress_dropped_audio` and `ingress_dropped_video`. Like `unkeyed`, the two counters use the shard's OWN snapshot, not the node merge (each shard publishes its own increments; taking them from the merged sum would count every shard's drops once per shard). The wave 61 handoff said the stats are "merged in `server.rs`": that merge is for the gauges and stays. Docs: the metric and its meaning (a modified client, or a cap constant too low if ordinary screen shares show it; no setting) plus the list of the other `sr_voice_*` metrics, in section 7 Monitoring.
- SEC-AUD-153: test `a_screen_share_over_its_cap_is_dropped_and_resumes` (screen cap 400 kbit/s, 20x frame size; a camera of another participant under its cap; a viewer). The test harness got a `screen_scale` field.

## Finding (test behaviour, no code change)
The screen path behaves like the camera path: frames over the cap are dropped and counted, the camera loses nothing and is not asked for keyframes, and forwarding resumes with a keyframe request. One detail: the keyframe request is made on the first forwarded frame after a drop. At about twice the cap the source alternates drop and pass, so each pass after a drop asks for a keyframe during the flood (the coalescer limits the rate); the test therefore ends the flood on a dropped frame before checking the request on the later resume. Nothing wrong in `forward_video`.

## Tests
- Run locally: `cargo clippy -j 4 -p sr-voice --all-targets -- -D warnings`; `--lib metrics` (3 pass); `--test video` and `--test music`.
- New: `metrics::tests::the_dropped_frame_delta_is_the_difference_since_the_last_publish`, `a_restarted_counter_does_not_underflow`, `the_counts_come_from_the_snapshot_fields`; `video.rs::a_screen_share_over_its_cap_is_dropped_and_resumes`.
- Not watched failing without the change: the unit tests and the screen test use `Counts`, `Snapshot::ingress_dropped_video` and `screen_scale`, so they cannot compile or exercise anything without the change; the screen test passes against the unchanged wave 61 `forward_video` (it documents behaviour). The publish step itself (`metrics::counter!` with the label) has no recorder test (the crate has no metrics-util dependency and none was added); it is two lines.

## Proposed inventory rows
- SEC-AUD-152: `sr_voice_ingress_dropped_frames_total{kind}` counts the frames the ingress caps drop (audio and video), published from each SFU shard's own counters; documented in doc 12 section 7. Decision D-639: no operator setting. Tests: `metrics::tests::*`.
- SEC-AUD-153: the screen-share ingress cap tested through real str0m clients: `a_screen_share_over_its_cap_is_dropped_and_resumes` (drop and count, camera unaffected, keyframe request on resume). Test only.

## Proposed threat-model sentence (D-631 row)
The residual "fixed constants with no operator setting" becomes: "fixed constants, by decision D-639; drops are visible in `sr_voice_ingress_dropped_frames_total`".

## Not done
Nothing from the task. No metric test with a recorder (see above).

## Owner questions
None.

## Cleanup
Cargo target `p62-voice-target` deleted at the end; no databases were created.
