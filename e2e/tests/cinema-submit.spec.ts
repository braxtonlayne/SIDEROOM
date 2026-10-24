import type { APIRequestContext, Page } from '@playwright/test';
import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { mkdtempSync, openSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { repoRoot, runCli } from '../support/cli';
import { apiPost, expect, test } from '../support/fixtures';

/**
 * Cinema > Submit in the browser (RDY-MED-002, RDY-MED-001): a member starts a series draft, drops a small
 * generated MP4 on the picker, the file name is read into season 1, episode 1, and the browser PUTs the part
 * straight to Garage with the presigned URL. The server completes the multipart upload from `ListParts`,
 * attaches the video to the draft episode, and the owner's asset list shows the stored bytes. A second test
 * breaks the part PUT, reloads the page and lets the IndexedDB ledger finish the upload.
 *
 * Runs in the `live` CI job (SR_E2E_LIVE=1: the real Garage stack). The first two tests use a tiny MP4 with no
 * decodable track, so where ffprobe is present the video ends "Failed" and, without it, at "Checking the file"; the
 * submission cannot be sent either way. The third test needs SR_E2E_FFMPEG=1 (the live job installs ffmpeg from
 * apt and builds sr-transcoder): it writes a real four-second MP4 with ffmpeg, starts an sr-transcoder worker
 * (SR_TRANSCODER_BIN) with an Owner-minted token, waits for "Ready", sends the submission and opens the
 * reviewer's proxy player.
 */

test.skip(
  process.env.SR_E2E_LIVE !== '1',
  'Needs the Garage stack the live CI job starts; set SR_E2E_LIVE=1.',
);

const API = '/api/v1';

/** One ISO base media file box. */
function box(type: string, ...payload: Buffer[]): Buffer {
  const body = Buffer.concat(payload);
  const head = Buffer.alloc(8);
  head.writeUInt32BE(8 + body.length, 0);
  head.write(type, 4, 'ascii');
  return Buffer.concat([head, body]);
}

const u32 = (n: number) => {
  const b = Buffer.alloc(4);
  b.writeUInt32BE(n, 0);
  return b;
};

/**
 * A tiny MP4 written by code (no ffmpeg): `ftyp`, a `moov` holding a version 0 `mvhd`, and an `mdat` of a few
 * kilobytes. It is a well-formed container with the right brands and a duration of one second, but it carries
 * no decodable track: enough for the upload path (size, container sniff, storage) and not for a transcode.
 */
function tinyMp4(extraBytes = 4096): Buffer {
  const ftyp = box('ftyp', Buffer.from('isom'), u32(512), Buffer.from('isomiso2avc1mp41'));
  const mvhd = box(
    'mvhd',
    u32(0), // version and flags
    u32(0), // creation time
    u32(0), // modification time
    u32(1000), // timescale
    u32(1000), // duration: one second
    u32(0x00010000), // rate 1.0
    Buffer.from([0x01, 0x00]), // volume 1.0
    Buffer.alloc(10), // reserved
    // The unity matrix.
    ...[0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000].map(u32),
    Buffer.alloc(24), // pre-defined
    u32(1), // next track id
  );
  const moov = box('moov', mvhd);
  const mdat = box('mdat', Buffer.alloc(extraBytes, 0x20));
  return Buffer.concat([ftyp, moov, mdat]);
}

async function apiGet<T>(request: APIRequestContext, path: string): Promise<T> {
  const res = await request.get(`${API}${path}`);
  if (!res.ok()) throw new Error(`GET ${path} -> ${res.status()}: ${await res.text()}`);
  return (await res.json()) as T;
}

/** The browser PUTs parts to the bucket from the app's origin: give the bucket that CORS rule (D-175). */
test.beforeAll(() => {
  try {
    runCli(['storage', 'ensure-cors']);
  } catch (e) {
    console.warn(`could not apply the bucket CORS rule (the stack may have done it): ${String(e)}`);
  }
});

const stamp = () => Date.now().toString(36);

/** Start a series draft in the UI and return its page. */
async function startSeries(page: Page, name: string) {
  await page.goto('/cinema/submit');
  await expect(page.getByRole('heading', { name: 'Submit a video' })).toBeVisible();
  await page.getByRole('button', { name: 'New submission' }).click();
  const form = page.getByRole('form', { name: 'New submission' });
  await form.getByRole('textbox', { name: 'Title' }).fill(name);
  await form.getByRole('button', { name: 'Start the draft' }).click();
  await expect(page.getByRole('heading', { name })).toBeVisible();
}

/** Put the generated file on the picker and start its upload. */
async function pickEpisode(page: Page, fileName: string, buffer: Buffer = tinyMp4()) {
  await page.getByLabel('Video files').setInputFiles({
    name: fileName,
    mimeType: 'video/mp4',
    buffer,
  });
  const proposal = page.getByRole('region', { name: 'How your files were read' });
  // The naming tester read season 1, episode 1 from "s01e01".
  await expect(proposal.getByRole('textbox', { name: `Season of ${fileName}` })).toHaveValue('1');
  await expect(proposal.getByRole('textbox', { name: `Episode of ${fileName}` })).toHaveValue('1');
  await proposal.getByRole('button', { name: 'Upload 1 video' }).click();
}

const row = (page: Page, label: RegExp) =>
  page.getByRole('list', { name: 'Videos of this submission' }).getByRole('listitem').filter({
    hasText: label,
  });

test('a member uploads a generated MP4 to a series draft and Garage holds the bytes', async ({
  seedUser,
  signedIn,
  owner,
}) => {
  const name = `E2E Night Shift ${stamp()}`;
  const fileName = `${name} - s01e01 - Pilot.mp4`;
  const member = await signedIn(await seedUser('submitter'));
  const page = member.page;
  await startSeries(page, name);
  await pickEpisode(page, fileName);

  // The draft episode appears, and its video leaves "No video yet" once the upload is complete and attached.
  const pilot = row(page, /S01E01 Pilot/);
  await expect(pilot).toBeVisible();
  await expect(pilot).toContainText(/Checking the file|Preparing for playback|Ready|Failed/, {
    timeout: 60_000,
  });
  await expect(pilot).not.toContainText('No video yet');

  // The owner sees the original in storage with the size that was announced.
  const staff = await signedIn(owner);
  const bytes = tinyMp4().length;
  await expect
    .poll(
      async () => {
        const list = await apiGet<{ items: { filename: string; original_size: number }[] }>(
          staff.context.request,
          '/admin/cinema/assets?limit=100',
        );
        return list.items.find((a) => a.filename.includes('s01e01'))?.original_size ?? null;
      },
      { timeout: 30_000 },
    )
    .toBe(bytes);

  // Without a real ffprobe the video is not playable, so the server says why it cannot be sent yet.
  await page.getByRole('button', { name: 'Send for review' }).click();
  // The shell's (empty) assertive live region is also role=alert; match the notice that carries the reason.
  await expect(
    page.getByRole('alert').filter({ hasText: /still being processed|no video yet/i }),
  ).toBeVisible();
});

test('an interrupted upload continues from the browser ledger after a refresh', async ({
  seedUser,
  signedIn,
}) => {
  const name = `E2E Resume ${stamp()}`;
  const fileName = `${name} - s01e01.mp4`;
  const member = await signedIn(await seedUser('resumer'));
  const page = member.page;
  await startSeries(page, name);

  // Every part PUT to the bucket fails: the manager retries, gives up and keeps the file in IndexedDB.
  const partPut = /partNumber=\d+/;
  await page.route(partPut, (route) => route.abort('failed'));
  await pickEpisode(page, fileName);
  const dock = page.getByRole('region', { name: 'Uploads' });
  await expect(dock).toContainText('Failed', { timeout: 60_000 });
  await expect(row(page, /S01E01/)).toContainText('No video yet');

  // After a refresh the ledger asks the server which parts arrived (none), sends the part and completes.
  await page.unroute(partPut);
  await page.reload();
  const resumed = row(page, /S01E01/);
  await expect(resumed).toContainText(
    /Checking the file|Preparing for playback|Ready|Failed|Uploading/,
    {
      timeout: 60_000,
    },
  );
  await expect(resumed).not.toContainText('No video yet', { timeout: 60_000 });
});

/**
 * A real four-second MP4 (H.264 and AAC) from ffmpeg's lavfi sources, so the probe, the review proxy and the
 * ladder all have something to decode. Only called where SR_E2E_FFMPEG=1 says ffmpeg is installed.
 */
function realMp4(dir: string): Buffer {
  const out = join(dir, 'source.mp4');
  const r = spawnSync(
    process.env.SR_FFMPEG ?? 'ffmpeg',
    [
      '-hide_banner',
      '-loglevel',
      'error',
      '-y',
      '-f',
      'lavfi',
      '-i',
      'testsrc=size=640x360:rate=25:duration=4',
      '-f',
      'lavfi',
      '-i',
      'sine=frequency=440:duration=4',
      '-c:v',
      'libx264',
      '-pix_fmt',
      'yuv420p',
      '-c:a',
      'aac',
      '-shortest',
      '-movflags',
      '+faststart',
      out,
    ],
    { encoding: 'utf8' },
  );
  if (r.status !== 0)
    throw new Error(`ffmpeg could not write the source: ${r.stderr || String(r.error)}`);
  return readFileSync(out);
}

/**
 * A real sr-transcoder worker (CPU path) for the duration of one test: the review proxy and the ladder are
 * transcode jobs, and sr-server only plans them. The worker token is minted by the Owner over the admin API.
 */
async function startWorker(staff: APIRequestContext, dir: string): Promise<ChildProcess> {
  const t = await apiPost<{ secret: string }>(staff, '/admin/transcode/tokens', {
    name: `e2e-cinema-${stamp()}`,
  });
  const tokenFile = join(dir, 'worker.token');
  writeFileSync(tokenFile, t.secret, { mode: 0o600 });
  const exe = process.platform === 'win32' ? '.exe' : '';
  const bin =
    process.env.SR_TRANSCODER_BIN ?? resolve(repoRoot, 'target', 'debug', `sr-transcoder${exe}`);
  const log = openSync(process.env.SR_E2E_WORKER_LOG ?? join(dir, 'worker.log'), 'a');
  return spawn(
    bin,
    [
      '--server',
      process.env.SR_E2E_WORKER_SERVER ?? 'http://127.0.0.1:8080',
      '--token-file',
      tokenFile,
      '--work-dir',
      join(dir, 'work'),
      '--name',
      'e2e-cinema',
      '--force-cpu',
    ],
    { stdio: ['ignore', log, log] },
  );
}

test.describe('with a real ffmpeg', () => {
  test.skip(
    process.env.SR_E2E_FFMPEG !== '1',
    'Needs ffmpeg and ffprobe on the runner and sr-transcoder built (SR_E2E_FFMPEG=1).',
  );

  test('the reviewer opens the proxy player of a submitted video', async ({
    seedUser,
    signedIn,
    owner,
  }) => {
    // A CPU proxy plus ladder on a standard runner takes minutes: the first CI run used up a 7-minute budget in
    // the "Ready" wait and closed the page as the reviewer opened the queue.
    test.setTimeout(900_000);
    const name = `E2E Proxy ${stamp()}`;
    const fileName = `${name} - s01e01 - Pilot.mp4`;
    const dir = mkdtempSync(join(tmpdir(), 'sr-e2e-cinema-'));
    const staff = await signedIn(owner);
    const worker = await startWorker(staff.context.request, dir);
    try {
      const member = await signedIn(await seedUser('proxy'));
      await startSeries(member.page, name);
      await pickEpisode(member.page, fileName, realMp4(dir));
      await expect(row(member.page, /S01E01 Pilot/)).toContainText('Ready', { timeout: 600_000 });
      await member.page.getByRole('button', { name: 'Send for review' }).click();
      await expect(member.page.getByText('Waiting for review').first()).toBeVisible();

      const queue = await apiGet<{ items: { id: string; title: string }[] }>(
        staff.context.request,
        `/admin/review/queue?q=${encodeURIComponent(name)}`,
      );
      const id = queue.items[0]?.id;
      expect(id).toBeTruthy();
      await apiPost(staff.context.request, `/admin/review/submissions/${id}/claim`, {
        takeover: false,
      });
      await staff.page.goto('/admin/review');
      await staff.page.getByText(name).first().click();
      await expect(staff.page.getByRole('heading', { name: 'Review proxy' })).toBeVisible();
      await expect(staff.page.locator('video').first()).toHaveAttribute('src', /\/cdn\//);
    } finally {
      // The worker never outlives the test, even when an assertion fails.
      worker.kill();
    }
  });
});
