import { apiPost } from '../support/fixtures';
import { expect, test } from '../support/world';

/**
 * RDY-PAR-005 (LNG-SSET-036): View Server As Role as a Lounge mode. In Server Settings > Roles the dialog works out,
 * with the shared `sr-permissions` WASM engine, which channels a member holding only the chosen role would see;
 * "Preview in the Lounge" carries the result into the Lounge, where the channel list of that server shows only
 * those channels under a banner with an exit button. Nothing is saved, and the exit puts the full list back.
 */

const VIEW_CHANNEL = '1024';

test('View Server As Role previews the Lounge with an exit banner', async ({ solo }) => {
  const { a, guildId } = solo;
  const page = a.page;
  const stamp = Date.now().toString(36);
  const roleName = `Reader ${stamp}`;
  const hidden = `staff-${stamp}`;
  await apiPost(a.context.request, `/guilds/${guildId}/roles`, { name: roleName });
  await apiPost(a.context.request, `/guilds/${guildId}/channels`, {
    name: hidden,
    type: 0,
    // @everyone (the role with the server's id) may not view it.
    permission_overwrites: [{ id: guildId, type: 0, allow: '0', deny: VIEW_CHANNEL }],
  });

  await page.goto(`/settings/server/${guildId}/roles`);
  await page.getByRole('button', { name: 'View Server As Role' }).click();
  const dialog = page.getByRole('dialog', { name: 'View Server As Role' });
  await dialog.getByRole('checkbox', { name: roleName }).check();
  const visible = dialog.getByRole('list', { name: 'Visible channels' });
  await expect(visible).toContainText('general');
  await expect(visible).not.toContainText(hidden);

  await dialog.getByRole('button', { name: 'Preview in the Lounge' }).click();
  const banner = page.getByTestId('view-as-banner');
  await expect(banner).toContainText(`Previewing`);
  await expect(banner).toContainText(roleName);
  const sidebar = page.getByTestId('channel-sidebar');
  await expect(sidebar).toContainText('general');
  await expect(sidebar).not.toContainText(hidden);

  // Exit: the banner goes, the settings page comes back, and the Lounge shows every channel again.
  await banner.getByRole('button', { name: 'Exit preview' }).click();
  await expect(page).toHaveURL(new RegExp(`/settings/server/${guildId}/roles`));
  await expect(page.getByTestId('view-as-banner')).toHaveCount(0);
  await page.goBack();
  await expect(page.getByTestId('channel-sidebar')).toContainText(hidden);
  await expect(page.getByTestId('view-as-banner')).toHaveCount(0);
});

/**
 * PAR-FU-001 (U-02): the preview also narrows the member list. In a channel only the previewed role may view, the
 * member who does not hold it is absent from the list (computed in the browser with the same engine); the Owner
 * stays. (Exiting is covered by the unit test; a full load also ends the preview.)
 */
test('View Server As Role hides members who cannot view the open channel', async ({ scenario }) => {
  const { a, b, guildId } = await scenario.ownerAndMember();
  const page = a.page;
  const stamp = Date.now().toString(36);
  const roleName = `Reader ${stamp}`;
  const role = await apiPost<{ id: string }>(a.context.request, `/guilds/${guildId}/roles`, {
    name: roleName,
  });
  const staff = await apiPost<{ id: string }>(a.context.request, `/guilds/${guildId}/channels`, {
    name: `staff-${stamp}`,
    type: 0,
    permission_overwrites: [
      { id: guildId, type: 0, allow: '0', deny: VIEW_CHANNEL },
      { id: role.id, type: 0, allow: VIEW_CHANNEL, deny: '0' },
    ],
  });

  await page.goto(`/settings/server/${guildId}/roles`);
  await page.getByRole('button', { name: 'View Server As Role' }).click();
  const dialog = page.getByRole('dialog', { name: 'View Server As Role' });
  await dialog.getByRole('checkbox', { name: roleName }).check();
  await dialog.getByRole('button', { name: 'Preview in the Lounge' }).click();
  await expect(page.getByTestId('view-as-banner')).toBeVisible();
  const members = page.getByRole('list', { name: 'Members' });
  // #general is open to everyone: both people show.
  await expect(members).toContainText(b.user.username);
  // A full page load would end the preview, so move to the staff channel through the sidebar.
  await page
    .getByTestId('channel-sidebar')
    .getByRole('link', { name: `staff-${stamp}` })
    .click();
  await expect(page).toHaveURL(new RegExp(staff.id));
  await expect(members).toContainText(a.user.username);
  await expect(members).not.toContainText(b.user.username);
});
