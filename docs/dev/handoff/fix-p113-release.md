# fix/p113-release (wave 113, builder "release"): SEC-AUD-561, 563, 564 (D-1043)

## Clock times
Started 02:23, all committed about 02:30 (clock read with `date`). T = 02:25.

## Done (commit)
- SEC-AUD-561: `.github/workflows/release.yml`, `publish` job, the notes step (was "Changelog from commits since previous tag", now "Release notes from the previous v tag"). Previous tag by `git describe --tags --abbrev=0 --match 'v*' "${tag}^"`; found: `git log` of `prev..tag`; not found (the first release): the top `## ` section of `CHANGELOG.md` plus the line "Full history: the commit log of this tag."; then a line-wise cap (`awk`, 99 900 characters of whole lines, then "(notes cut at 100 000 characters)", 100 000 at most); an empty result becomes "Release <tag>."; the step prints the character count. Every command that can fail is guarded (`|| true`), so the step cannot fail. The file is still written to `CHANGELOG.md` (a runner file, never committed), so the "Publish the release" step is unchanged (`--notes-file CHANGELOG.md`).
- SEC-AUD-564: `--locked` on the `cargo zigbuild` line of `binaries` (`release.yml:29`).
- SEC-AUD-563: `LABEL org.opencontainers.image.source="https://github.com/braxtonlayne/SIDEROOM"` right after the final `FROM` of the `server`, `voice`, `transcoder` and `transcoder-cpu` Dockerfiles. The URL is the tree's own: `git remote -v`, and the run links in `docs/13-roadmap-and-phases.md:13` (`https://github.com/braxtonlayne/SIDEROOM/actions/...`). No placeholder exists, so no build argument and no change to the `images` job. (A fork building its own images would link to this repository; four lines to change.)
- Not changed: what the jobs build, the three web checks and their order, the tag trigger, `release-dry-run.yml`, the "Publish the release" step.

## Not done
Nothing outstanding.

## FINDERS (git grep for `release.yml`, `zigbuild`, `org.opencontainers` across .github scripts e2e docs deploy)
- `release-dry-run.yml:47,54` and `scripts/pack-web-release.mjs:6` mention `release.yml` in comments about the web job only; untouched.
- `docs/12-deployment-and-operations.md:27`, `docs/01-decisions-log.md:946`, `docs/dev/owner-checklist.md:39-40`, `docs/inventory/21-feature-phase.md:1006-1009` name the file or the rows; no sentence about the notes text or the label is made false. `server.Dockerfile:22` already has `--locked`. No test reads the notes step today.

## CASE FOR VERIFY
(a) Read `.github/workflows/release.yml`, job `publish`: the step whose `run` contains `git describe` also contains `--match 'v*'` and a cap (`99900`), and writes `CHANGELOG.md`; the step "Publish the release" passes `--notes-file CHANGELOG.md` to `gh release create`. Would have caught: the old step (no `--match`, no cap) giving 265 000 characters and no release page.
(b) The `run:` of the `binaries` job that starts `cargo zigbuild` contains `--locked`. Would have caught SEC-AUD-564.
(c) Every `deploy/docker/*.Dockerfile` has a line `LABEL org.opencontainers.image.source=` after its last `FROM`. Would have caught SEC-AUD-563 (and a new Dockerfile added without it).

## The written walk
First release `v0.1.0` (no earlier `v*` tag; `p27-day1` is not `v*`, so it is ignored): `binaries`, `web`, `images` run as before (zigbuild now `--locked`: a stale `Cargo.lock` fails the archive job as it already fails the image). `publish`: download, Checksums, then the notes step: `git describe --match 'v*' v0.1.0^` prints nothing, so `prev` is empty; `awk` takes the top `## ` section of the tagged commit's `CHANGELOG.md` (about 1 000 to 2 000 characters), adds a blank line and the "Full history" line, the cap does not cut, the log prints "release notes: N characters". `gh release create v0.1.0 release/* --notes-file CHANGELOG.md --verify-tag` makes the page. Second release `v0.1.1`: `prev` = `v0.1.0`; the notes are the log of `v0.1.0..v0.1.1`; over 99 900 characters it is cut on a line boundary with the last line saying so. The images carry the source label, so ghcr links each package to the repository (visibility stays the owner's).
Assumptions the first tag tests: `fetch-depth: 0` brings the tags (already set); `gh release create` accepts a body of up to 100 000 characters (GitHub's limit is believed to be 125 000: not checked).

## Proof (local, no tag, no remote, 02:24 to 02:27)
The step's `run` text was parsed out of `release.yml` with js-yaml and run unchanged with `bash -eo pipefail`, `GITHUB_REF_NAME=HEAD` (`CHANGELOG.md` restored with `git checkout --` after each run):
- steps of `publish`, in order: `actions/checkout@v4 | actions/download-artifact@v4 | Checksums | Release notes from the previous v tag | Publish the release`.
- (a) as it is (no `v*` tag): 1069 characters (= bytes), 9 lines. First line `## Phase 112 - 2026-11-19`; last lines: the Phase 112 bullet "For the team: ...", two blank lines, `Full history: the commit log of this tag.`
- (b) `git describe` stood in by a function returning `p27-day1` (a range of over 3000 commits): 99 927 characters (= bytes), 1365 lines; first line `- docs: wave 112 D-1038, rows SEC-AUD-561..574 ...`; last line `(notes cut at 100 000 characters)`, the line before it a whole commit line ending `(3ac233b1)`.
- (c) stand-in returning `HEAD` (empty range): 14 characters, one line `Release HEAD.`
- `npx prettier --check .github/workflows/release.yml`: exit 0. `strays.sh p113-release`: none found.

## FOR THE COMMANDER: reading CI
`release-dry-run.yml` does not run the notes step, `publish` or the binary archives: nothing in CI proves 561 or 564 before the first tag. The three local runs above are the proof for 561. The LABEL lines are read by the dry run's server-image build (`release-dry-run.yml`) and by `prod-smoke.yml` if it builds an image: a bad LABEL line would fail that `docker build` on the pull request. If the notes step were wrong, the failure would show in the first tag's `publish` log (the step is built not to fail).

## What I could not prove
GitHub's real body limit (believed 125 000) and `gh release create` itself; the first tag is the test. The release report's section 6 fallback (make the page by hand from the `bin-*` run artifacts) is now only unlikely to be needed, not unnecessary: the notes step cannot fail and is capped, but `gh` and the other steps of `publish` are as before.

## Proposed rows / sentences for other people's files
None. Closing texts:
- SEC-AUD-561: "The release notes are capped at 100 000 characters, start at the previous `v*` tag, and for the first release are the top section of CHANGELOG.md; the step cannot fail (D-1043). Proven by three local runs; the first tag is the live test."
- SEC-AUD-563: "All four Dockerfiles under `deploy/docker/` carry `org.opencontainers.image.source` pointing to the repository, so ghcr links each package to it. Package visibility stays the owner's to set after the first push."
- SEC-AUD-564: "The `cargo zigbuild` line of `binaries` has `--locked`, as the images' builds have."

## Owner questions (default chosen)
Should a fork's images link to its own repository? Default: no, the constant is this repository (a build argument would be needed otherwise).

## PROCESSES
`strays.sh p113-release`: none found. I started no process that outlives a command.
