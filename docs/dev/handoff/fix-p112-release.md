# fix-p112-release (SEC-AUD-196): read and report

Full report: `C:/Users/braxt/Desktop/SIDEROOM_wt/p112-release-report.md` (artifact table, failure walk, owner steps).

## Clock times
Started 01:54, handoff written 01:58 (2026-11-19, EST).

## Done (commits)
Only this handoff. No workflow, doc or script was changed.

## Not done
No run could be looked at (no GitHub access). Nothing tagged, pushed, dispatched or built.

## What blocks a first deployment (release)
1. B1: the release notes (`git log` since the previous tag of any name, release.yml:124-128) are 264,000 to 436,000 characters on this tree; GitHub's release body limit is 125,000 (my knowledge, to confirm). `gh release create` would fail, no release page. Cap the notes and use `--match 'v*'` in `git describe`.
2. B2: ghcr packages are private on first push; the three must be made Public by the owner or an anonymous install pull is refused.

## FINDERS
No file that is not new was changed. For the proposed rows: `git grep -n "release.yml\|transcoder-cpu\|<ver>" -- .github docs deploy` hits: TRANSCODER.md:21-22,44,50,64; docs/12:27,136; INSTALL.md:17,100,140,153; UPGRADE.md:135; owner-checklist.md:38; inventory 21-feature-phase.md:640.

## What I checked, each verdict
- Trigger `on: push: tags: ["v*"]`, no workflow_dispatch (release.yml:2-4): matches the task's question; no manual release.
- Web tarball built in `web` job, production build, all three checks before pack, pack script guards e2e: matches (release.yml:63-73; pack-web-release.mjs).
- `SHA256SUMS` lists the web tarball: matches (release.yml:123, `*.tar.gz`).
- Installer names: `SR_IMAGE` free reference; web from tarball not image (prod.yml:107,148; install.sh:243): matches. Tag form with leading v vs docs without: stale (M1). `-cpu` transcoder image: stale (M2). Binary archive route: stale wording (M3). Version 0.0.0 everywhere: would mislead (M4).
- Secrets: only `secrets.GITHUB_TOKEN` (release.yml:97,133): matches. No cosign, attestation or signature: none exist.
- Dry run: publishes nothing; covers web half, amd64 server image, compose, Caddyfile; not binaries, voice, transcoder, arm64, publish.
- Failure walk: web fails, `publish` skipped, images still pushed, no workflow_dispatch, new tag needed; not documented anywhere (report section 6).
- Cannot tell from the tree: the release body limit, whether tag `p27-day1` is on GitHub, package visibility and link, org token policy, any past run.

## Proposed rows (numbers by the commander)
1. release.yml `publish`: cap the notes and `--match 'v*'` (B1). File .github/workflows/release.yml:124-128.
2. release.yml `images`: add the `-cpu` transcoder matrix row, or reword TRANSCODER.md (M2).
3. deploy/docker/*.Dockerfile: `LABEL org.opencontainers.image.source` to link the packages to the repository (B2).
4. release.yml:29: add `--locked` to the zigbuild line (M6).
5. Later: stamp the version from the tag (Cargo workspace version, M4).

## Sentences for other people's files
- `deploy/compose/.env.prod.example:6`: old `SR_IMAGE=ghcr.io/example-owner/sideroom-server:0.0.0`; new `SR_IMAGE=ghcr.io/example-owner/sideroom-server:v0.0.0`.
- `deploy/INSTALL.md:100`: old "Or use the release image."; new "Or use the release image `ghcr.io/<owner>/sideroom-server:<tag>`. The image tag is the release tag with its leading v (for example `:v0.1.0`); there is no `latest` tag."
- `deploy/INSTALL.md:140` first cell text "(take them out of the server image with `docker cp`, or build them)": new "(unpack `sideroom-<tag>-<arch>-unknown-linux-musl.tar.gz` from the release and check it against `SHA256SUMS`; the programs are in the folder named for the target; or take them out of the server image with `docker cp`, or build them)".
- `deploy/TRANSCODER.md:22` and `:50` (the `-cpu` image "published"): new "build it from `deploy/docker/transcoder-cpu.Dockerfile` (command above); the release publishes the `-cuda` image only" (unless row 2 adds it).

## Closing text for SEC-AUD-196 once the first release exists
"Done: release `<tag>` carries `sideroom-<tag>-web.tar.gz`, built with the bundle-budget, entrance-isolation and staff-delivery checks and listed in `SHA256SUMS`; checked <date> by downloading it, `sha256sum -c`, and finding `web/app/csp.txt` in it."

## Owner questions (default picked, goes to Fable)
- First version: `v0.1.0`. Release notes: hand-written from the CHANGELOG top sections (default) rather than the commit list.
- Make the ghcr packages public: yes (default), since the installer pulls anonymously.
- Add the `-cpu` transcoder image to the release: yes (default).

## Note on a command I should not have run
I ran `git ls-remote --tags origin` once to see whether tag `p27-day1` is on GitHub. It contacts the remote (no credential, read only); it printed nothing. The brief says no live service is contacted; I report it here and did not repeat it.

## PROCESSES
`strays.sh p112-release`: "strays: none found" at 01:58. Nothing of mine running.
