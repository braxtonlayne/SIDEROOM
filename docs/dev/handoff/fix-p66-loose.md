# Wave 66 loose (SEC-AUD-190..193; D-665 client half, D-669, D-670, D-672)

Branch `fix/p66-loose` (base `integration/phase66`). Web only: no Rust, no migration, no `openapi.json`. Nothing pushed.

## Done (commits, oldest first)
1. `SEC-AUD-190` (3904d2ce): the Gateway reference page is `gateway-events`, the Gateway guide opens at `/developers/docs/gateway` and links to it in its first block (new block kind `pagelink`); `@sideroom/bot` docs say it is a workspace package used from the repository (no registry) with the real run command; `/library/upload` redirects to `/library/submissions` under `ingestQueues` (the Shell's separate Upload entry and `SoonPage` are removed); the `methods` search parameter is gone from `public/auth/router.tsx` (nothing read it after D-557). Specs: `entry-landing-and-login.md` (no glimpse link), `library-reader.md`, `developer-platform.md`.
2. `SEC-AUD-191` (d44c5414): `gateway/client.ts` keeps `reconnects` (connections opened after the first, a RESUME counts) and `heartbeatMs` (send to ACK, `performance.now()`), no timer or listener; `bugreport/sources.ts` reads them through `getGateway()`; `bundle.ts` clamps reconnects at 10,000 (the server's limit). Doc 04 states the fields.
3. `SEC-AUD-192` (2f4febde): "Server Support" and "App Directory" stubs removed (and the `ComingSoon` dialog of `GuildHeader`); welcome unchanged (3 cards); LNG-CHN-009 age-restricted mark (`alert` icon over the channel icon, `aria-label` "age-restricted", also in the link's label); LNG-INV-007 "Grant temporary membership" switch (hint matches `take_temporary_memberships`: removed when the last session ends unless they hold a role; the invite list marks temporary ones); XC-PWA-001 Live TV shortcut (`app-manifest.webmanifest` is the session-gated file served as `/app/manifest.webmanifest`, so D-385 allows it; `pwa.test.tsx` pins it). Specs: `lounge-chat.md`, doc 10.
4. `SEC-AUD-193` (three commits): LNG-GLD-023 Call in the member menu; LNG-HOME-011 emoji in the custom status (the API already carries `custom_status.emoji_name`; Unicode emoji only, picker in the dialog, shown in the user panel, popout and profile); LNG-RAIL-014 NEW badge (`lounge/newServer.ts`: own `joined_at` under 24 h and not opened; "opened" kept in `localStorage` key `sr.lounge.openedServers`); stale comments (A12): `entrance.css`, `render-legal.mjs`, `developers/routes.tsx`, `landing.a11y.test.tsx`.
5. `e2e/tests/p66-loose-shots.spec.ts` (local-only, `SR_P66_LOOSE=1`).

## Not done / left
- **LNG-GLD-018 (Directory link in the join dialog): not committed.** caps owns `AddServerDialog.tsx` and had no commit there yet; I have no message channel to the commander, so the patch and its test are at the end of this file. It needs the key `lounge.addServerDialog.browseDirectory`, which is already in `p66-loose-1.ts`. There is no per-server "directory module" switch in the client (the rail's Discover button is always on), so the link is always shown.
- Composer's disabled "Share from Cinema / Library (coming soon)": left on purpose, they name planned work.
- `admin/cinema/tv/ChannelEditor.tsx:2` comment ("Logo upload and the guide import are not on the server yet; the screen says so"): catalog owns `admin/**`; I did not check it against the code. Please check and reword.
- Non-web A12 items (`sr-voice-proto` wire.rs, `bridge.rs:130`) untouched.
- Doc outside my files: `docs/dev/entrance-isolation.md` section 7 (legal page paragraph) says "token custom properties only" for `legal-*.css`; the sheet also has two `@font-face` rules and page rules. Replace with "its own custom properties, the two fonts and the page rules".

## First paint
`node scripts/check-bundle-budget.mjs`: before (`p65-integ` dist, the base) `app-DPphO_rI.js` 214,601 bytes gzip, room 41,399; after `app-D4yTH539.js` 214,722 bytes (+121), room 41,278 (limit 250 KB, needs 8 KB). Bundle budget and `check-entrance-isolation.mjs` pass (every public surface clean).

## Keys I stopped using (still in existing fragments)
`lounge.channelSidebar.serverSupport`, `lounge.channelSidebar.appDirectory`, `lounge.channelSidebar.serverActionSoon` (loungeA-2.ts); `library.shell.nav.upload` (library-2.ts); `library.pages.soon.requests.title`/`.text`, `library.pages.soon.lists.title`/`.text`, `library.pages.soon.upload.title`/`.text`, `library.pages.soon.title` (library-3.ts). My fragment: `i18n/messages/en/p66-loose-1.ts`.

## Tests
New or changed (single files): `developers/__tests__/docsIds.test.tsx` (`every_doc_page_id_is_unique`, guide opens and links to the reference; both watched fail with the old id), `packages.test.tsx` (id), `library/__tests__/submissions.test.tsx` (redirect; hidden with the switch off), `routes/entrance/passkeys.test.tsx` (validateSearch keeps no `methods`), `gateway/gateway.test.ts` (reconnects 0/1/2, heartbeat null then 40), `bugreport/__tests__/bundle.test.ts` (clamp, absent), `lounge/__tests__/serverFlows.test.tsx` (no stub entries; temporary switch sends `temporary: true`), `ageGate.test.tsx` (mark), `pwa/pwa.test.tsx` (Live TV), `p66MemberCall.test.tsx` (Call; hidden for bot and blocked), `homeFlows.test.tsx` and `p11ShellProfile.test.tsx` (status emoji), `p66NewServer.test.tsx` (badge). All pass; web typecheck, eslint and prettier clean on the files; `check-doc-links` ok. Whole suites left to CI. The `.md` specs I edited were already not prettier-clean on the base.

## Shots
`visual-sweep-shots/p66-loose` (desktop 1280x800 and phone 375x812, Editorial, Lounge Light, Lounge Dark; I looked at them): developer docs guide, reference and SDKs; `/library/upload` landing on submissions; sidebar with the age-restricted channel, server menu and invite dialog with the switch (desktop only: the phone opens on the channel view); welcome (three cards); member menu with Call (desktop); rail NEW badges; custom status dialog and result; report dialog with "What is sent" open (the preview shows the top of the JSON; the spec asserts `"reconnects"` is in it; `heartbeat_ms` appears after the first ACK and is not asserted); sign-in page signed out with `?methods=passkey`. One lock round (web build, cargo build of sr-server and sr-cli, the two checks, stack, Playwright chromium); the spec waits 31 s between guild creations because of the create-guild rate limit. Scratch DB `sr_p66_loose_ui` dropped, `p66-loose-target` deleted, stack stopped by PID, lock released.

## Inventory rows
Closed by this work, with evidence: LNG-CHN-009 (`ChannelSidebar.tsx` `ChannelGlyph`, `ageGate.test.tsx`), LNG-INV-007 (`InviteDialog.tsx`, `serverFlows.test.tsx`), XC-PWA-001 (`app-manifest.webmanifest`, `pwa.test.tsx`), LNG-GLD-020 and LNG-GLD-021 (stubs removed, `serverFlows.test.tsx` "no coming soon entries"; the welcome keeps its three cards), LNG-GLD-023 (`MemberList.tsx`, `p66MemberCall.test.tsx`), LNG-HOME-011 (`CustomStatusDialog.tsx`, `homeFlows.test.tsx`), LNG-RAIL-014 (`newServer.ts`, `p66NewServer.test.tsx`; relies on the member's own `joined_at` being in the store: in the stack the badge showed on servers created and not opened). LNG-GLD-018 stays open until the patch lands.

Proposed rows:
- SEC-AUD-190 [x] Developer docs: unique page ids (the reference is `gateway-events`), `@sideroom/bot` described as repository-only, `/library/upload` redirects to submissions, `methods` search parameter removed (D-672, D-557).
- SEC-AUD-191 [x] The debug bundle carries the gateway reconnect count and the last heartbeat round trip (D-665, D-649).
- SEC-AUD-192 [x] Server menu stubs removed; age-restricted channel mark, temporary-membership invite switch, Live TV shortcut (D-670).
- SEC-AUD-193 [x] Call in the member menu, emoji in the custom status, NEW server badge from `joined_at`, stale comments (A12).

Threat-model rows: none.

## Changelog sentence
The debug report now includes how often the connection dropped and how fast the last heartbeat came back; the server menu lost two "coming soon" entries; invites can grant temporary membership; age-restricted channels, new servers and custom-status emoji show in the Lounge; member menus can start a call; the developer docs' Gateway guide opens again.

## Owner questions
- Custom status takes Unicode emoji only (the server also accepts a custom `emoji_id`; not offered). OK?
- "NEW" on a server clears when the member opens it (per browser); Discord uses a fixed time. Keep the open rule?
- "Call" in the member menu rings the person at once, as the DM header's button does, with no confirmation.

## The `AddServerDialog.tsx` patch (LNG-GLD-018, not committed)
Apply to `apps/web/src/lounge/AddServerDialog.tsx`, and add the test below as `lounge/__tests__/p66JoinDirectory.test.tsx`.

```diff
diff --git a/apps/web/src/lounge/AddServerDialog.tsx b/apps/web/src/lounge/AddServerDialog.tsx
index 6726e6f8..a1823fc2 100644
--- a/apps/web/src/lounge/AddServerDialog.tsx
+++ b/apps/web/src/lounge/AddServerDialog.tsx
@@ -9,6 +9,7 @@ import { useStore } from '../store';
 import type { Channel as StoreChannel } from '../store';
 import { listBuiltinTemplates, type BuiltinTemplate } from '../store/rest';
 import { useCurrentUser } from './data';
+import { useLoungeUi } from './store';
 import { inviteErrorMessage, parseInviteCode } from './inviteFlow';
 
 /** Navigate to a guild (and a channel in it when known). */
@@ -40,6 +41,7 @@ export function AddServerDialog({ onClose }: { onClose: () => void }) {
   const [purpose, setPurpose] = useState<Purpose>(null);
   const me = useCurrentUser();
   const goto = useGotoGuild();
+  const openDialog = useLoungeUi((s) => s.openDialog);
   const [name, setName] = useState(
     me.displayName ? t('lounge.addServer.defaultName', { name: me.displayName }) : '',
   );
@@ -308,6 +310,9 @@ export function AddServerDialog({ onClose }: { onClose: () => void }) {
               <li>example.com/invite/cool-people</li>
             </ul>
           </div>
+          <Button variant="ghost" onClick={() => openDialog({ kind: 'directory' })}>
+            {t('lounge.addServerDialog.browseDirectory')}
+          </Button>
           <div className="lng-form-actions">
             <Button variant="ghost" onClick={() => go('choose')}>
               {t('lounge.addServerDialog.back')}
```

```tsx
// LNG-GLD-018: the join dialog links to the Server Directory (wave 66, loose).
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { at, baseHandlers, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

describe('join dialog: Browse the Server Directory (LNG-GLD-018)', () => {
  it('opens the directory in place of the join dialog', async () => {
    baseHandlers();
    seedLounge({ messages: [] });
    server.use(
      http.get('*/api/v1/directory/categories', () => HttpResponse.json(['gaming'])),
      http.get('*/api/v1/directory', () => HttpResponse.json([])),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add a Server' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Join a Server' }));
    await userEvent.click(screen.getByRole('button', { name: 'Browse the Server Directory' }));
    expect(await screen.findByRole('dialog', { name: /Discover|Directory/i })).toBeInTheDocument();
    expect(screen.queryByLabelText('Invite link or code')).toBeNull();
  });
});
```
