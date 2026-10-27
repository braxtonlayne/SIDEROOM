# Questions for a lawyer

> **Not legal advice; questions for a lawyer.** This file is a hand-over list. It does not say what the law requires, and nothing in it has been checked by a lawyer. Each question quotes the decision it came from in [the decisions log](../01-decisions-log.md); the full wording of the point to check is in that decision's "Not legal advice" or "Lawyer note" text, and the log wins if this file and the log ever differ.

Every default below is the project's own, taken "by default (owner-delegated)" unless the log says otherwise. A default stands until the lawyer says otherwise. For each question this file gives: the question in plain words, the default and where it is set, and what changes if the answer is different. Settings named here are edited in **Admin > Features > Compliance** (feature `platform.compliance`, a reason is required, D-254) unless another place is named.

## About the instance (the preamble to hand over)

- **What it is.** A private, invitation-only platform for a group of friends and a few related communities ([doc 00](../00-vision-and-scope.md)). Registration needs a platform invite. It is built for up to 20,000 accounts; real use is expected under about 100. It is not offered to the public and has no public sign-up page.
- **Who runs it.** One operator, who is also the owner. The operator's country is a platform setting (`compliance.operator_country`, unset until the owner sets it, asked in the Setup Wizard) and is shown on the privacy page with the operator contact (`compliance.operator_contact`).
- **What is stored, and for how long.** The full table is [doc 09 section 1](../09-retention-and-privacy.md); the points a lawyer usually asks about are:

| Data | Kept |
|---|---|
| Messages | Forever, unless deleted; a deleted message is purged after 30 days; a message under a moderation hold is not purged |
| Large attachments | 7 days after upload |
| Accounts | Forever, even after erasure, in the form stated to the member |
| Sign-in addresses (`login_events`) | 180 days, deleted by a daily job |
| Invoices | `compliance.invoice_retention_years` (default 10) after the year of payment, then detached from the member |
| Invite graph, staff access log | Forever |
| Voice and video | Never recorded or stored; end-to-end encrypted |

- **Where the notices live.** The copyright notice address is `compliance.operator_contact`, published on `/help`. Third-party licence texts are in [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md), linked from `/help`.
- **Not built, by decision.** Mailing the templated takedown reply and forwarding a counter-notice (N-35) and any mandatory external reporting step are not built; they wait for the answers to Q9, Q13 and Q16.

## Part A. Software licences

### Q1. RAR reading and the UnRAR licence (D-287)

- **Question.** Is a decompress-only use of the vendored UnRAR source, with the licence text in the notices file, within the RARLAB licence, and does placing that text in the image and on a web page count as accompanying the software?
- **Default and where.** RAR/CBR reading is on: `library.ingest.rar_enabled` (platform settings, default on). The licence text is in `THIRD_PARTY_NOTICES.md`, copied to `/usr/share/doc/sideroom/` in the image and served at `/THIRD_PARTY_NOTICES.md`.
- **If the answer differs.** Turning the setting off refuses RAR/CBR uploads at ingest and removes the exposure; the notices entry can stay. A different placement is a documentation and release-workflow change only.

### Q2. JASSUB under LGPL-2.1 (D-297)

- **Question.** The four checks in the "Not legal advice" note of D-297: section 6 for a minified application chunk that imports an LGPL library, the FreeType Licence choice being recorded, the licence of the bundled default font, and the build-time replacement not being a modification.
- **Default and where.** `jassub@2.5.16`, unmodified, as its own lazily loaded chunk with the worker and `.wasm` served as separate unminified files; texts in `THIRD_PARTY_NOTICES.md`; a licence allow-list step in `.github/workflows/web.yml`.
- **If the answer differs.** Without JASSUB the player falls back to WebVTT conversion of ASS subtitles, losing typesetting; removing it is one dependency and one loader branch.

### Q3. The GPL polyfill and distribution of the web bundle (D-309)

- **Question.** Whether serving the web bundle (which includes the GPL-3.0 `rvfc-polyfill`) to people outside the operator is a distribution, and what the operator must offer if the instance is ever opened beyond the private group.
- **Default and where.** The real polyfill ships; the earlier empty-module replacement is dropped. Nothing is offered today because the instance is private. `rvfc-polyfill` is named in the allow-list of `web.yml` and in `THIRD_PARTY_NOTICES.md`.
- **If the answer differs.** Either a corresponding-source offer for the web bundle is added to `/help`, or the empty-module replacement that the log records as superseded is restored (older browsers then get WebVTT).

### Q4. JASSUB's licence expression and its default font (D-314)

- **Question.** The two points in the "Not legal advice" note of D-314: one OFL 1.1 text carrying two headers, and the WOFF2 conversion upstream not attaching to the project.
- **Default and where.** `THIRD_PARTY_NOTICES.md`, section "Liberation Sans 2.00.5"; the stricter reading of the `package.json` expression is followed.
- **If the answer differs.** Edit the notices file (and run `node scripts/check-notices-copy.mjs --write`); if the font is a problem, a different default font is a one-file change in the JASSUB loader.

## Part B. Age

### Q5. Minimum age and self-declaration (D-294)

- **Question.** The digital-consent age where the instance runs, whether self-declaration is enough, and whether reset-and-restrict for 13 to 17 is right rather than parental consent.
- **Default and where.** `accounts.minimum_age` = 13, range 13 to 18; self-declared on the invite landing; no identity check.
- **If the answer differs.** A higher age is a setting change, no release. Parental consent or identity checks would be new work and a new privacy trade-off.

### Q6. The separate age checkbox (D-311)

- **Question.** Whether a separate, unticked box is an acceptable form of the declaration (the addition D-311 makes to Q5).
- **Default and where.** Two required, unticked boxes on the entrance form ("house notes" and "at least {age}"); the age is read from `accounts.minimum_age`.
- **If the answer differs.** Wording or box structure changes are entrance-form copy and a test change.

### Q7. Underage accounts: suspension, erasure and the parent's route (D-341)

- **Question.** The three points in the lawyer note of D-341: whether actual knowledge of an under-13 obliges deletion rather than suspension where the instance runs, how long the operator may wait for a parent, and whether the parent's identity must be verified before the account is kept.
- **Default and where.** Suspension plus an `erasure` request in the data-request queue (Admin > Data requests), due by `compliance.data_request_deadline_days`, never run automatically; the under-18 system DM names the reason and points at `compliance.operator_contact`.
- **If the answer differs.** The erasure can be made immediate or declined per request in the queue; a different waiting period is the deadline setting; a parent-verification step would be new work.

## Part C. Copyright notices and illegal content

### Q8. The copyright process as a whole (D-293)

- **Question.** The three checks in the "Not legal advice" note of D-293, for the operator's jurisdiction: whether a US DMCA section 512 agent registration is wanted, what DSA articles 16 and 17 require of the mechanism and the statement of reasons, and whether the ten-business-day wait matches local law. Q9 to Q16 take up the parts that were later sharpened; this question covers what is left.
- **Default and where.** Staff-entered notices (Admin > Moderation > Takedowns); `moderation.strike_review_threshold` = 3 within `moderation.strike_window_months` = 12, which opens a review case and never suspends automatically; the notice address on `/help` is `compliance.operator_contact`.
- **If the answer differs.** The threshold and window are settings. A public notice form would be new work.

### Q9. Designated agent (D-336 (a))

- **Question.** Whether to register a designated agent with the US Copyright Office so the registered agent and the `/help` contact match.
- **Default.** Not registered; `/help` shows `compliance.operator_contact`.
- **If the answer differs.** Registration is an outside step; afterwards set `compliance.operator_contact` to the registered address.

### Q10. Notice-and-action mechanism and the kind of service (D-336 (b))

- **Question.** Whether a published mailbox is acceptable without a public form, whether the notifier must receive an acknowledgement and a decision by mail, and whether SIDE ROOM is a hosting service or an "online platform" under the DSA (with the consequences listed in (b)).
- **Default.** Mailbox only; staff copy replies by hand; the mail path is the unbuilt N-35.
- **If the answer differs.** A "yes, mail is required" builds N-35; an "online platform" answer adds the article 24(5), 20 and 23 duties as new work.

### Q11. Statement of reasons (D-336 (c))

- **Question.** Whether the uploader DM meets the statement of reasons, including the points of article 17(3) that (c) lists.
- **Default.** The DM carries the notice reference, the claimed basis cut at 300 characters and the counter-notice path, never the notifier's identity.
- **If the answer differs.** DM template change in the takedown module (wording and fields).

### Q12. The ten business days (D-336 (d))

- **Question.** Whether the restore wait of section 512(g)(2)(C) is read correctly for the operator's country, and whether a jurisdiction without a statutory counter-notice wants a wait at all.
- **Default and where.** Exactly 10 business days, in UTC, on the operator country's weekend, no holiday calendar: the constant `RESTORE_BUSINESS_DAYS` in `crates/sr-moderation/src/takedown_rules.rs`.
- **If the answer differs.** A code constant change (not a setting), with its tests.

### Q13. Forwarding the counter-notice (D-336 (e))

- **Question.** Whether safe harbour needs the counter-notice forwarded to the notifier, and which of the uploader's details the staff form must capture and may pass on.
- **Default.** The register records the counter-notice and forwards nothing.
- **If the answer differs.** The mail path of N-35 is built; the staff form gains the fields the lawyer names.

### Q14. Repeat-infringer policy wording (D-336 (f))

- **Question.** The wording, approved by a lawyer, under which the strike rule and the contested-strike rule appear in the house notes or on `/help`.
- **Default.** The rule exists as built; the public wording is not yet approved.
- **If the answer differs.** Copy change in the house notes and the Help page, in every shipped locale.

### Q15. Notifier data: basis, retention and export (D-336 (g))

- **Question.** The lawful basis and retention period for a notifier's personal data, and whether the CSV export should redact the notifier fields by default.
- **Default.** Notices, notifier contact details and uploader DMs are kept forever and exported on request (CSV, logged).
- **If the answer differs.** A retention job and an export option for redaction are new work.

### Q16. Mandatory reporting and preservation (D-336 (h))

- **Question.** Which reporting or preservation duty applies to what an evidence hold may contain, in the operator's country (for a US operator, the item (h) names).
- **Default.** The hold keeps the stored object; the register has no reporting step.
- **If the answer differs.** A reporting step and a preservation period become new work; the owner also says whether mandatory external reporting is wanted at all.

## Part D. Personal data

### Q17. Data-subject requests and retained records (D-295)

- **Question.** The lawful basis for each retained record, whether a records-of-processing document is needed, and whether the operator must name an EU representative or register with a supervisory authority.
- **Default and where.** `compliance.data_request_deadline_days` = 30 (range 7 to 90); the retained list stated to the member after erasure; the operator's country and contact on the privacy page.
- **If the answer differs.** The deadline is a setting; the retained list and the privacy copy are text; a records-of-processing document is a new document.

### Q18. Restriction of processing (D-343)

- **Question.** The two points in the lawyer note of D-343: whether a restriction may be limited to the discovery processing while the member stays visible in their own servers, and whether the notice meets article 18(3) when the member cannot be reached.
- **Default.** The restriction tool (Admin > Data requests) hides shared cinema and library activity, presence, aggregates and recommendations; the member gets a system DM when it is applied and when it is lifted.
- **If the answer differs.** A broader restriction would also hide the member from member lists and search, which is new work and changes what other members see.

### Q19. Sign-in address retention and the evasion review (D-347)

- **Question.** The three points in the "Not legal advice" note of D-347: whether 180 days is defensible for the operator's country, whether the signal rows and the confirmed snapshot are covered by the same interest, and whether the privacy summary should name the evasion review.
- **Default and where.** `IP_WINDOW_DAYS` = 180 in `crates/sr-moderation/src/suspicion_rules.rs`, enforced by the daily job `moderation.suspicion_housekeeping`; no hold exception; a confirmed signal keeps counts and dates, never an address.
- **If the answer differs.** The window is a code constant; naming the review in the privacy summary is copy in doc 09 and the privacy page.

## Part E. Features of the feature phase (roadmap-p27, section 20)

These four were added by wave 28 (byos-legal) from [the roadmap](../dev/roadmap-p27.md) section 20, so the lawyer sees them with the rest. Q20 is built in wave 28 (BYOS-LEG-001); the features behind Q21 (the Discord bridge, wave 35), Q22 (metadata providers, waves 29 and 30) and Q23 (the emulator, waves 36 to 38) are not built yet and run on the defaults below. Q24 and Q26 belong to the deferred game servers (D-383) and are kept in the roadmap until wave 38 is resumed.

### Q20. User-owned buckets: brokered access, takedown and delisting (BYOS-LEG-001, D-380)

- **Question.** Friend and guild shelves mean the platform brokers access to media it does not store (scoped, short-lived presigned links to a member's own bucket). Does brokering make the operator a host or an intermediary for that content? Is delisting (hiding the catalog rows and refusing the links, because the bytes belong to the member and deleting them is not in the operator's power) an adequate response to a notice? Does a whole-mount delisting, applied by staff, change the answer?
- **Default and where.** The takedown register has the item kind `storage_object` (`crates/sr-moderation/src/takedowns.rs`, migration 1460). An upheld notice delists the object (`crates/sr-storage/src/scopes.rs` refuses the key, whoever asks, the owner too), the owner gets the same direct message as an uploader (D-336 (c)), a counter-notice restores on the existing ten-business-day clock, and the strike counts against the owner's Person as for an upload. Staff may delist a whole mount (status `delisted`, reason required, the owner told why). A platform ban or a suspension refuses every mount of the account until it is lifted. A data-subject export lists the member's mounts (labels, buckets, counts, shares; no keys) and erasure soft-deletes them and wipes the sealed secret keys. The operator's own instance bucket is mount 1, so nothing changes for staff-uploaded media.
- **If the answer differs.** If brokering makes the operator a host, a notice must also stop the presign path for the whole bucket and not only the key (a setting in code); if deletion were expected, the platform would need write credentials to member buckets, which the design deliberately avoids.

### Q21. The Discord bridge privacy notice (BRG-UI-001, D-381)

- **Question.** Mirroring sends members' names, avatars (a 24-hour signed URL) and messages to Discord Inc. and shows Discord users' names and messages here. Are a channel banner, a system message and the privacy page an adequate notice? Is a per-member opt-out enough, or is consent needed before a channel is mapped? What retention applies to the message map?
- **Default and where.** Banner, system message, privacy page and per-channel opt-out; the module is off on every server and only the server owner switches it on; map rows are kept with the messages; erasure deletes the webhook messages the bridge posted. Built in wave 35.
- **If the answer differs.** Consent before mapping adds a prompt to each member of a mapped channel; a shorter map retention is a job setting.

### Q22. Metadata-provider terms (BYOS-CAT-001, MUS-IDX-001)

- **Question.** The lookup calls TMDB, AniList, MangaDex, MangaUpdates and ComicVine, and wave 30 adds MusicBrainz (no key, one request a second, a user agent naming the instance). TMDB's terms allow free use for non-commercial purposes and require attribution: does an instance that sells Patron tiers count as commercial? Do the terms of MangaDex and ComicVine allow caching covers?
- **Default and where.** Attribution text and logo on every page that shows fetched metadata (`lookup.attribution` on the row); covers cached as thumbnails only; each provider can be switched off with `review.lookup.enabled`.
- **If the answer differs.** A commercial finding means a commercial TMDB agreement or turning TMDB off; cover caching is a job setting.

### Q23. Emulator cores and BIOS (ACT-EMU-001, ACT-EMU-003)

- **Question.** The cores are GPL-2.0, GPL-3.0 and MPL-2.0, served as separate files, and the web bundle is served to members, so the distribution caveat of D-309 applies in full (the corresponding-source offer of Q3 would now cover ffmpeg.wasm and the cores). Is a link to each project's source plus the exact build set an adequate offer? The PS1 BIOS is Sony's copyright and is never shipped or stored (the member supplies it from their own bucket and it is hash-checked): does hash-checking a member's BIOS create any exposure?
- **Default and where.** No BIOS on the instance; the shell refuses unknown hashes; the member handbook says only ROMs the member owns, or homebrew, are legal to play. Built in waves 36 to 38.
- **If the answer differs.** A stricter finding removes the PS1 core or the hash check; the source offer is text on `/help`.

## Considered, no question

D-351 (a demoted automatic case stays listed in the suspicion panel) changes only where staff find the case. It stores nothing that Q19 does not already cover, so it adds no question for the lawyer.
