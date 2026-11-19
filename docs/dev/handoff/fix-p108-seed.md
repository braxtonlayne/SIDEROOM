# fix-p108-seed (wave 108, seed: D-1026, SEC-AUD-529)

## Clock times
Started 20:33. Last job ended 21:04. Handoff 21:07.

## Done (commits on fix/p108-seed)
- `e2e/support/p100-seed.ts`: new `seedSweepExtras(...)` (+ `SweepExtras` type). Every step prints `ok`/`FAIL` into the seed log.
- `e2e/tests/page-sweep.spec.ts` ("seed the sweep world" only): after the Study switches it calls `seedSweepExtras` with the Owner's and the member's sessions and rewrites `world.json` as `{ ...world, extras }`. `SweepWorld` gets an optional `extras`. No route, shot or other test touched; the old keys are unchanged.
- `e2e/tests/p108-seed-shots.spec.ts` (new): opens each addition at 375 touch (Lounge Light), asserts it by its recorded id, one shot each, checks the old world is intact.
- `e2e/support/world.ts`: not changed (nothing needed).

## What the world now holds (`world.json` key `extras`)
| key | what |
|---|---|
| friendId, friendName | account `sweepfriend_<stamp>`, display name "Marlow": accepted friend of the Owner |
| askerId, askerName | account `sweepasker_<stamp>`, "Tess": pending incoming request (the Owner sees Accept) |
| groupDmId, groupDmName | group DM "Sweep X crew <sfx>" of Owner + Marlow + Tess, one message each (type 3) |
| extrasChannelId, extrasChannelName | text channel `sweep-x-<sfx>` in the sweep guild (new, so the seeded channel gets no message) |
| reactionsMessageId | message there with 14 reactions (3 with count 2): 3 lines at 375 |
| spoilerMessageId | message there with a spoiler |
| spoilerCommentId, spoilerCommentTitleId | the member's comment, spoiler flag set, on the world's Cinema title |
| listId, listName | private Library list "Sweep X night picks <sfx>", empty (shows the empty-state link) |
| notSeeded | the list below, with reasons |

Register limit: the seed registered 1 account already, 2 added = 3 a minute (limit 5; `registerUser` waits out a 429).
A group DM needs friends, so the requester is first a friend, the group is made, then the Owner removes the friendship and Tess asks again (the group stays; Pending shows her).

## Not done / could not be seeded (and what each would need)
- Attachments and images on a message: the upload slot works but the PUT goes to a fake `memory://` URL and message create refuses the missing object (probe in the spec: `UPLOAD_INVALID`, file not uploaded). Needs real object storage in the cell (an S3/MinIO bucket).
- A Study file: files are objects on a storage shelf; needs the same bucket plus an uploaded file.
- A call in progress: needs a media server (LiveKit) setting in the cell.
- A playable video (player bar): a media file in storage.
- A Live TV channel on the air: scheduled programs with playable media.
- A bucket for the tier image picker: a bucket setting.
- epub and pdf readers: a Library upload (a file in storage).
- Old sweep shots that change a little (unavoidable): `/channels/@me` gains a group DM, the Friends page has a friend and a request, the sweep guild has one more text channel at the end of its list (and one more row in Server settings > Channels), the title's Comments tab has one more comment, the Library has one list, Admin > Accounts has two more accounts. The seeded channel and every old key are unchanged.

## REACH table
Not applicable (tools builder).

## FINDERS (rule 13)
`git grep -n "p100-seed"`: `p100-seed-shots.spec.ts`, `p101-arcade-shots.spec.ts`, `p101-trans-shots.spec.ts` import `seedActivityTier` / `seedTranscodingRow`, untouched; only the fixtures import line of `p100-seed.ts` was widened. `loadWorld` in `page-sweep.spec.ts` ignores unknown keys; keys were added, none changed. No app file changed.

## JOBS
All with `--fresh`, class shoot. Mode `base` (the `after` mode needs a build and my tree has no app change; 0253 was submitted as `after` and failed rc 3 `stale build`).
| id | submitted | started | ended | rc | note |
|---|---|---|---|---|---|
| 0253 | 1795052262 | 1795052517 | 1795052518 | 3 | stale build (wrong mode) |
| 0255 | 1795052525 | 1795052569 | 1795052615 | 1 | seed FAIL: a group DM needs friends |
| 0257 | 1795052640 | 1795052762 | 1795052804 | 1 | seed FAIL: group DM rename went to the guild route (403) |
| 0260 | 1795052817 | 1795052986 | 1795053059 | 0 | spec problems: Friends button has a badge, group DM read by the wrong route, message count assumption |
| 0266 | 1795053086 | 1795053258 | 1795053317 | 0 | no problem lines; the cinema shot was above the comment |
| 0272 | 1795053330 | 1795053762 | 1795053820 | 0 | final: no problem lines, 7 shots |
Waiting was long (up to 432 s for 0272); running 46 to 73 s.

## Seed time
Final run, from the job log: the whole "seed the sweep world" test took 32.1 s (my spec 11.6 s). The extras step logs its own time: 12 s. So the base seed is about 20 s and mine adds 12 s. Well under 3 minutes. A true base twin is not possible with `--fresh` (the seed comes from my tree), so the figure is total minus extras.

## Old world intact (how)
Not by a base twin but by assertions in the spec on the fresh world: the 13 `world.json` keys the base seed wrote are all present, no unexpected key besides `extras` and `seedLog`; the second text channel `announcements-and-very-long-channel-name` and the voice channel are there; the seeded channel holds 13 messages (the base seed posts 8, the a11y world 3, plus server notices; I never post to it) and none of the new texts; every earlier seed step still prints `ok` (40 `ok`, no `FAIL`). The problems list in `figures-after.json` is empty.

## Shots (opened after the job; `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-seed`, 375 x 812 touch, Lounge Light)
- `friends-all.png`: Friends, All tab: one row, Marlow, with Message and More buttons; Pending badge 1 in the tab bar. No sideways scroll (375/375).
- `friends-pending.png`: Pending tab: "Incoming - 1", Tess with accept and decline buttons.
- `group-dm.png`: the group DM, title "Sweep X crew ...", three messages by Rowan, Marlow, Tess.
- `reactions-wrapped.png`: the new channel: the message with 14 pills over three lines.
- `spoiler-message.png`: same channel, the message with the spoiler bars.
- `cinema-comment-spoiler.png`: title page, Comments tab: "This contains spoilers. Show spoiler" row, an ordinary comment, the tombstone.
- `library-list.png`: list page: "Only me", the list name, Edit/Delete list, empty state "Nothing here yet" with a Browse series button.

## Who imports my files
`p100-seed.ts`: e2e specs only. `page-sweep.spec.ts`, `p108-seed-shots.spec.ts`: Playwright. No app file, no lazy chunk.

## Checks
- `npx prettier --check` on the three e2e files: pass (21:03).
- e2e `npx tsc --noEmit -p tsconfig.json`: exit 0 (before each submit).
- vitest, eslint, web tsc, i18n audit, doc-link check: left to the commander.

## Pages a second census should measure next wave (way in)
At 375 touch (and 1024 with `reach: 'box'`), on a world seeded from the merged tree; ids from `world.json` `extras`.
1. Friends, All tab (Message, More actions): `/channels/@me`, the "Friends" button, tab All.
2. Friends, Pending tab (accept, decline): same page, tab Pending.
3. The group DM page and header: `/channels/@me/<groupDmId>`; the New Group DM dialog from Friends.
4. The wrapped reactions line and the add-reaction button: `/channels/<guildId>/<extrasChannelId>`.
5. A spoiler in a message: same channel.
6. Cinema comment with a spoiler (Show spoiler, Like, Reply, Contains spoilers): `/cinema/t/<spoilerCommentTitleId>`, tab Comments.
7. Library list with the empty-state links and Browse series: `/library/lists/<listId>` (and `/library/lists`).
8. The forward and search dialogs of MessageDialogs (the two `.lng-emoji-search`): from a message's menu in the extras channel and the channel search.
9. Admin > Accounts (two more accounts), where the `.ss-select` selects are: `/admin`.

## Closing text for SEC-AUD-529 and proposed new rows
Closing: "Partly closed in wave 108 (D-1026, seed): the sweep world gains a friend, a pending request, a group DM, a 14-reaction message and a spoiler message in a new channel, a spoiler comment on the Cinema title and an empty Library list, all under `world.json` `extras`. Still not seedable: message attachments and images, a Study file, a call, a playable video, a Live TV channel on the air, the tier image bucket, the epub and pdf readers (all need object storage or a media server in the cell). Handoff `fix-p108-seed.md`."
Proposed new row: [QA] Low: give the job cell real object storage (for example MinIO) so attachments, Study files, the readers and the tier picker can be seeded; needs a decision on the cell's memory footprint.

## Changelog sentence
The visual-sweep world now seeds a friend, a pending request, a group DM, a wrapped-reactions message, a spoiler comment and a Library list so the next census can measure them.

## Owner questions
None. (Default taken: the new data lives beside the old, in a new channel, not in the seeded channel.)

## PROCESSES
strays: none found
