import { apiPost, expect, test } from '../support/fixtures';
import { dropServer } from '../support/world';
import {
  LINKS_NOT_TABBABLE,
  expectHeard,
  focusedName,
  heard,
  isFocused,
  tabTo,
  tabsToLinks,
  watchLive,
} from '../support/keys';

/**
 * RDY-A11Y-003, flow 3: the Lounge with the keyboard only. Skip link to the main region, the server rail and the
 * channel list as roving lists (arrows move, Enter opens), the message list as a roving list, and a message from
 * someone else is announced politely without taking focus.
 */
// The server is removed again whether the test passed or not (support/world.ts says why).
let made: Parameters<typeof dropServer>[0] | null = null;
test.afterEach(async () => {
  if (made) await dropServer(made);
  made = null;
});

test('lounge: servers, channels and messages by keyboard; a new message is announced and focus stays', async ({
  scenario,
}) => {
  const { a, b, guildId, channelId, guildName } = await scenario.ownerAndMember();
  made = { a, guildId };
  const page = a.page;
  test.skip(!(await tabsToLinks(page)), LINKS_NOT_TABBABLE);
  const second = await apiPost<{ id: string }>(a.context.request, `/guilds/${guildId}/channels`, {
    name: 'second',
    type: 0,
  });
  for (const text of ['first line', 'second line', 'third line'])
    await apiPost(a.context.request, `/channels/${channelId}/messages`, { content: text });

  await watchLive(page);
  await page.goto(`/channels/${guildId}/${channelId}`);
  const log = page.getByRole('log');
  await expect(log.getByRole('article').filter({ hasText: 'third line' })).toBeVisible();
  const before = await log.getByRole('article').count();

  // Skip link: first stop, Enter lands on <main>.
  await page.keyboard.press('Tab');
  await expect(page.getByRole('link', { name: 'Skip to main content' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('main')).toBeFocused();

  // Server rail: one tab stop, arrows move between servers, Enter opens the one with focus.
  const rail = page.getByRole('navigation', { name: 'Servers' });
  const stops = rail.locator('a[tabindex="0"]');
  await expect(stops).toHaveCount(1);
  // Shift+Tab from <main> walks back through the user panel and the channel list to the rail.
  await tabTo(page, stops, { max: 40, back: true });
  const first = await focusedName(page);
  await page.keyboard.press('ArrowDown');
  expect(await focusedName(page)).not.toBe(first);
  await page.keyboard.press('Home');
  expect(await focusedName(page)).toBe(first);
  const here = rail.getByRole('link', { name: guildName });
  for (let i = 0; i < 80 && !(await isFocused(here)); i++) await page.keyboard.press('ArrowDown');
  await expect(here).toBeFocused();
  await expect(rail.locator('a[tabindex="0"]')).toHaveCount(1);
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(new RegExp(`/channels/${guildId}`));

  // Channel list: arrows between channels, Enter opens.
  const channels = page.getByRole('navigation', { name: /channels/i });
  const general = channels.getByRole('link', { name: /general/ });
  const next = channels.getByRole('link', { name: /second/ });
  await tabTo(page, general, { max: 12 });
  const start = await focusedName(page);
  await page.keyboard.press('ArrowDown');
  expect(await focusedName(page)).not.toBe(start);
  await page.keyboard.press('ArrowUp');
  expect(await focusedName(page)).toBe(start);
  // The new channel may sit above or below #general: walk to it, then open it with Enter.
  for (let i = 0; i < 8 && !(await isFocused(next)); i++) await page.keyboard.press('ArrowDown');
  for (let i = 0; i < 8 && !(await isFocused(next)); i++) await page.keyboard.press('ArrowUp');
  await expect(next).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(new RegExp(`/channels/${guildId}/${second.id}`));
  // Alt+Up and Alt+Down switch to the previous or next channel from anywhere (docs/dev/a11y-checklist.md).
  await page.keyboard.press('Alt+ArrowDown');
  if (!page.url().endsWith(channelId)) {
    await page.keyboard.press('Alt+ArrowUp');
    await page.keyboard.press('Alt+ArrowUp');
  }
  await expect(page).toHaveURL(new RegExp(`/channels/${guildId}/${channelId}`));
  await expect(log.getByRole('article').filter({ hasText: 'third line' })).toBeVisible();

  // Messages: one tab stop, arrows move, the position holds when someone else writes.
  const rows = log.getByRole('article');
  await tabTo(page, rows.first(), { max: 40 }).catch(() => tabTo(page, rows.last(), { max: 40 }));
  await page.keyboard.press('Home');
  await expect(rows.first()).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(rows.nth(1)).toBeFocused();
  expect(await isFocused(rows.nth(1))).toBe(true);
  await apiPost(b.context.request, `/channels/${channelId}/messages`, {
    content: 'hello from the second member',
  });
  await expect(log.getByRole('article')).toHaveCount(before + 1);
  await expectHeard(page, /hello from the second member/);
  await expect(rows.nth(1)).toBeFocused();
  expect((await heard(page, 'assertive')).length).toBe(0);
});
