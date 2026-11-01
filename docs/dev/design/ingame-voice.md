# In-game voice: the media leg of the lobby sub-room (design note, not built)

Status: **designed, not built.** ARC-GST-001 (wave 43) builds the lobby's *data* leg and reserves the manifest flag
`ingame_voice`. This note says where the media leg attaches, so that adding it later is additive: no change to the
guest socket's isolation rule, to the lobby's keys, or to any game that does not use voice.

Decisions it serves: D-385 (the only pre-sign-in surface is the isolated runner), D-397 (a guest hears and sees only
what their own game session produces), D-399 (a developer gets in-game voice by declaring a capability, not by a
platform rework), the Immutable Rule (end-to-end encryption is never toggleable), and the owner's design requirement of
wave 43: *proximity chat inside the game, carried on the lobby's sub-room with its keys, never the voice channel's MLS
audio and never anyone's call outside the game.*

## What exists today (the data leg)

* `sr_voice::lobby` holds one `LobbyHub`: a room per `LobbyId` (the SHA-256 of the lobby token), seats with a role
  (`Member` or `Guest`), a per-seat byte budget, bounded queues, and `LobbyKeys { key_id, epoch }` created when the
  first seat arrives and forgotten with the last. `Scope { data: true, media: MediaScope::Reserved }`.
* The lobby shares **no state and no type** with `SfuCore` (no `ParticipantId`, `ChannelId`, MLS epoch, SFrame key).
* The guest socket (`sr_gateway::guest`, `GET /gateway/guest`) carries opaque `data` frames between seats on its one
  topic `game:<lobby_token>`. The relay holds no key: players encrypt under a key derived from the link's `#key=`
  fragment, which the server never sees (label `sideroom/lobby/v1/data`).

## The media leg

### Manifest

`ingame_voice` is declared with `lobby_channel` (voice needs a lobby). The validator already accepts it. At runtime it
stays `FlagStatus::Reserved`: `arcade::is_active("ingame_voice")` is false, so an instance never lists it in
`capabilities`, and the runner (member shell and guest runner) refuses to start a voice leg. When the capability
ships, only that status and the runner switch change.

### Transport

A lobby seat opens a **separate WebRTC connection to the same SFU process, to a lobby room, not to a voice channel**:

* New signalling route on the guest socket (new ops, same one topic): `media_offer` / `media_answer` /
  `media_candidate`. Nothing on the member gateway changes.
* New core type `LobbyMedia` next to `LobbyHub` in `sr-voice`, built on the same str0m `Rtc` plumbing as `SfuCore`
  but with its own table of participants keyed by `(LobbyId, Seat)`. It forwards audio only (no video, no screen share,
  no data-channel instance registry); it reuses the forwarding, loss and keyframe modules, not the room registry.
* ICE/UDP ports are the call SFU's, so no new listener. A lobby participant is never added to a voice channel room and
  a voice channel member is never added to a lobby by being in a call: the only way into a lobby room is a lobby
  seat, which is a lobby token or a member's seat on the same activity.

### Keys

Media is end-to-end encrypted (SFrame), like the call, but under **lobby keys, not MLS**:

* Players derive the media key from the same `#key=` fragment as the data key, with the distinct label
  `sideroom/lobby/v1/media` (`sr_voice::lobby::MEDIA_KEY_LABEL`; a unit test pins that no lobby label looks like an MLS
  exporter label). The SFU never sees it.
* `LobbyKeys.key_id` and `epoch` are announced in the socket's `ready` frame today. For media, a **rekey** (a seat
  leaves, the host revokes a link) bumps `epoch`; each client re-derives `HKDF(key, "…/media" || epoch)`. A guest who
  was revoked therefore cannot decrypt audio sent after the rekey, even if they keep the old key.
* The SFrame sender id is the **seat number**, not a user id: no account id or handle reaches a guest.

### Scope and isolation (what a guest hears)

* A seat hears only the other seats of the same lobby. Proximity is the game's business: the game sends positions in
  data frames and the client mixes/attenuates; the SFU forwards every audio track of the lobby to every seat (the
  lobby is at most 32 seats), optionally with a game-chosen *audible set* per seat sent as a data hint so a large lobby
  does not forward everything (`LobbyMedia::set_audible(seat, &[Seat])`, a hint the relay enforces for forwarding but
  that is not a security boundary: all seats share one key anyway).
* A guest never receives the channel's audio: the lobby room has no path to the call room, and the guest socket holds
  one topic (enforced at the relay; see `crates/sr-gateway/src/guest_tests.rs`).
* A member who is in the call and in the game has **two separate connections**: the call (MLS) and the lobby media.
  The runner mixes the lobby into the game only, and never sends call audio into the lobby or lobby audio into the call
  (the guest bridge capability set has no `callAudio` and the member shell does not route it).

### Limits

* Audio only, Opus, a per-seat bitrate ceiling (`lobby.voice_kbps`, a ceiling that only shrinks), at most 32 seats,
  and the same budget idea as `Budget` for any data frames.
* A lobby revoke or end closes the media room together with the data room (`LobbyHub::close` is the one call).
* Abuse: a guest can be muted by the lobby host (a data frame the relay enforces by not forwarding the seat's track)
  and a revoked link is closed within the guest socket's recheck interval.

### What it needs before it is built

1. `LobbyMedia` in `sr-voice` and the three signalling ops (about the size of the data leg).
2. An SFrame implementation for lobby keys in the runner (the call's `sr-e2ee` SFrame code is MLS-exporter keyed; the
   lobby needs the same cipher with a key from the fragment: a small wrapper, not a new cipher).
3. A guest-runner microphone permission prompt (the isolated runner bundle must be allowed `getUserMedia`; the
   sandbox's `allow` list is extended by the capability, never by default).
4. A threat-model row for voice in the lobby (who can hear a guest: only the lobby), and tests: a guest cannot decrypt
   after a rekey, a lobby never forwards to a call room, `ingame_voice` still refused when the flag is off.

## Why this does not break the isolation rule

The rule is enforced by the relay and the lobby's types, not by the SDK. The media leg adds a *kind of payload* to a
room that already cannot see anything else. It adds no way to name a channel, a guild, an account or an MLS group, and
it does not widen the one-topic socket: media signalling is additional ops on the same topic. The runner refuses the
capability today (`ingame_voice` is reserved), and an old or hostile client that tries `media_offer` gets the same
403/Drop as any unknown op, because the relay only knows the ops it lists.
