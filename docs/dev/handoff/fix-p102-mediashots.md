# Wave 102, mediashots (shots builder, pod media): SEC-AUD-453, D-958

Interim, 09:58. Spec `e2e/tests/p102-media-shots.spec.ts`. Round 1 (base and after in one stack round, base = the pod's 7 app files put back to `b308d5e2` from git, then the merged text restored): 09:51 to 09:56, slot 2 ("mediashots 09:50"), released 09:56.

## FAULTS (send back; the cure is "that select goes back to raw")
1. **watch, `apps/web/src/watchparty/ControlBar.tsx`, the Speed select** (`aria-label` Speed, in `label.wp-inline`, inline-flex): narrower than the base at all three section configs: 72 px -> 58 px (1024 shipped, 375 shipped, 1024 light); height 31 -> 40 (44 at 375) is fine. The bar's row re-centres (Play, Back, Forward and Position move up 2 px, the select 6.5 px). Cause: the shrink-to-fit inline-flex label with the kit's 100 % wrapper. Put back to raw.
2. **media, `apps/web/src/music/panel/TrackList.tsx`, "Add {title} to a playlist"** (in the flex-wrap `.sr-mlib-more` row): 214 x 29 -> 506 x 40 at 1024 (321 x 44 at 375). The select leaves the row of buttons and takes a line of its own: every row below moves 42 px down where the height difference allows 11 (7 controls, both Lounge themes at 1024). Put back to raw.

No fault: `PeoplePanel.tsx` (both: 335 wide, 31 -> 40 / 44, labels +9 / +13, chevron clear of the text), `StartPartyDialog.tsx` (520 x 40; at 375 the base select ran 27 px past the dialog's edge and the kit's fits: after is 303 wide against 330, that is the base's overflow gone, not a narrowing), `FollowButton.tsx` (360 x 36 -> 40), `SearchBox.tsx` (sort 103 x 28 -> 153 x 40, `msg.css:779` does not win over the kit's height; the bar grows 12 px; 44 not measured, no phone search box).

Pending a second round: TitlePicker Season (not reached in round 1: an empty party cannot be made, the spec now reaches it through the Queue tab), FollowButton at 375 (behind "More actions" on a phone; spec fixed), the base shot of the theatre at 375 (the base page showed "You are not in this party any more": spec now rejoins).
