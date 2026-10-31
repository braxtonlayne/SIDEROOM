import type {
  APIRequestContext,
  Browser,
  BrowserContext,
  FrameLocator,
  Page,
} from '@playwright/test';
import {
  apiPatch,
  apiPost,
  apiPostBytes,
  loginApi,
  registerUser,
  type SeededUser,
} from './fixtures';
import {
  debugPage,
  joinVoice,
  mediaContext,
  trackPeerConnections,
  voiceServer,
  type VoiceServer,
} from './voice';

/** One side of a two-person Activity session: the media context, its page and the Activity's frame. */
export interface ActivitySide {
  context: BrowserContext;
  page: Page;
  frame: FrameLocator;
}

export interface ActivitySession {
  /** The application's name (the frame title is `<name> (Activity)`). */
  name: string;
  appId: string;
  server: VoiceServer;
  /** Alice: started the Activity, so she is its host. */
  a: ActivitySide;
  /** Bob: joined the running instance. */
  b: ActivitySide;
  close(): Promise<void>;
}

export const activityFrame = (page: Page, name: string): FrameLocator =>
  page.frameLocator(`iframe[title="${name} (Activity)"]`);

/** The Activity bar above the call (Join / Leave / End for everyone). */
export const activityPanel = (page: Page, name: string) =>
  page.getByRole('complementary', { name: `${name} Activity` });

/**
 * Install `bundle` as a released Activity and run it with two people in one voice call, the way a server
 * installs a built-in Activity (P36-ACT-4): Bob registers an application and uploads the zip, the Owner
 * approves the version and the bundle (the `datachannel` capability is part of that review), Alice starts it
 * from the call and Bob joins the instance. Resolves once both frames have loaded.
 */
export async function startActivityForTwo(
  fx: {
    browser: Browser;
    browserName: string;
    owner: SeededUser;
    playwright: { request: { newContext(o: { baseURL: string }): Promise<APIRequestContext> } };
  },
  bundle: Buffer,
  label: string,
): Promise<ActivitySession> {
  const base = process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173';
  const req = await fx.playwright.request.newContext({ baseURL: base });
  const alice = await registerUser(req, `${label}a`);
  const bob = await registerUser(req, `${label}b`);
  await req.dispose();
  const staff = await fx.playwright.request.newContext({ baseURL: base });
  await loginApi(staff, fx.owner);

  const a = await mediaContext(fx.browser, fx.browserName, alice);
  const b = await mediaContext(fx.browser, fx.browserName, bob);
  await trackPeerConnections(a.page);
  await trackPeerConnections(b.page);
  debugPage(a.page, 'A');
  debugPage(b.page, 'B');
  const close = async () => {
    await a.context.close();
    await b.context.close();
    await staff.dispose();
  };

  try {
    const name = `E2E ${label} ${Date.now().toString(36)}`;
    const { application: app } = await apiPost<{ application: { id: string } }>(
      b.context.request,
      '/applications',
      { name, description: `Built-in Activity (${label}), e2e copy` },
    );
    await apiPatch(b.context.request, `/applications/${app.id}`, {
      privacy_policy_url: 'https://example.test/privacy',
      terms_of_service_url: 'https://example.test/terms',
    });
    const version = await apiPost<{ id: string }>(
      b.context.request,
      `/applications/${app.id}/versions`,
      { note: 'e2e' },
    );
    await apiPost(staff, `/admin/developers/versions/${version.id}/decision`, {
      decision: 'approve',
    });
    const up = await apiPostBytes<{ bundle: { id: string; capabilities?: string[] } }>(
      b.context.request,
      `/applications/${app.id}/activity/bundles`,
      bundle,
      'application/zip',
    );
    if (!up.bundle.capabilities?.includes('datachannel'))
      throw new Error(`the ${label} bundle does not declare the datachannel capability`);
    await apiPost(b.context.request, `/applications/${app.id}/activity/submit`, {
      bundle_id: up.bundle.id,
    });
    await apiPost(staff, `/admin/developers/activities/${app.id}/decision`, {
      decision: 'approve',
      bundle_id: up.bundle.id,
    });

    const server = await voiceServer(a.context, [b.context], `${label} night`);
    await joinVoice(a.page, server);
    await joinVoice(b.page, server);
    await a.page.getByRole('button', { name: 'Start an Activity' }).first().click();
    await a.page.getByRole('button', { name: `Start ${name}` }).click();
    await a.page.locator(`iframe[title="${name} (Activity)"]`).waitFor({ timeout: 30_000 });
    await b.page
      .getByRole('button', { name: `Join ${name}` })
      .first()
      .click({ timeout: 30_000 });
    await b.page.locator(`iframe[title="${name} (Activity)"]`).waitFor({ timeout: 30_000 });
    return {
      name,
      appId: app.id,
      server,
      a: { ...a, frame: activityFrame(a.page, name) },
      b: { ...b, frame: activityFrame(b.page, name) },
      close,
    };
  } catch (e) {
    await close();
    throw e;
  }
}
