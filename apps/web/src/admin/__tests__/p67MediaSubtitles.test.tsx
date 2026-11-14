// SEC-AUD-201 (D-685): the Subtitles block of the Cinema asset panel: the list, each edit sends its PATCH, one
// default at a time, hide or show, upload and each refusal, delete after a confirmation.
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { ApiError } from '../../api/client';
import { asStaff, http, HttpResponse, readJson, server } from './helpers';
import type { SubtitleTrack } from '../cinema/api';

const upload = vi.hoisted(() => vi.fn());
vi.mock('../cinema/api', async (orig) => ({
  ...(await orig<typeof import('../cinema/api')>()),
  uploadSubtitle: upload,
}));

import { SubtitlesPanel } from '../cinema/SubtitlesPanel';

const track = (over: Partial<SubtitleTrack> & { id: string }): SubtitleTrack => ({
  label: 'English',
  language: 'en',
  forced: false,
  hearing_impaired: false,
  is_default: false,
  hidden: false,
  source: 'embedded',
  styled: false,
  ...over,
});

const TRACKS = () => [
  track({ id: '1', label: 'English', is_default: true }),
  track({ id: '2', label: 'Español', language: 'es', hidden: true }),
  track({ id: '3', label: 'Deutsch', language: 'de', source: 'uploaded' }),
  track({
    id: '4',
    label: 'Signs',
    language: 'en',
    source: 'uploaded',
    styled: true,
    forced: true,
  }),
];

interface Calls {
  patches: { id: string; body: Record<string, unknown> }[];
  deletes: string[];
}

function useSubs(initial: SubtitleTrack[] = TRACKS(), o: { patchStatus?: number } = {}) {
  const calls: Calls = { patches: [], deletes: [] };
  let tracks = initial;
  server.use(
    http.get('*/api/v1/admin/cinema/assets/:id/subtitles', () => HttpResponse.json({ tracks })),
    http.patch('*/api/v1/admin/cinema/assets/:id/subtitles/:track', async ({ request, params }) => {
      const body = (await readJson(request)) as Record<string, unknown>;
      calls.patches.push({ id: String(params.track), body });
      if (o.patchStatus)
        return HttpResponse.json(
          { code: 50035, message: 'language must be a BCP 47 tag such as en or pt-BR' },
          { status: o.patchStatus },
        );
      if (body.is_default === true) tracks = tracks.map((t) => ({ ...t, is_default: false }));
      tracks = tracks.map((t) => (t.id === params.track ? { ...t, ...body } : t));
      return HttpResponse.json(tracks.find((t) => t.id === params.track));
    }),
    http.delete('*/api/v1/admin/cinema/assets/:id/subtitles/:track', ({ params }) => {
      calls.deletes.push(String(params.track));
      tracks = tracks.filter((t) => t.id !== params.track);
      return new HttpResponse(null, { status: 204 });
    }),
  );
  return calls;
}

async function open(languages: string[] = ['en', 'es', 'de']) {
  asStaff({ perms: ['STAFF_BASIC', 'PUBLISH_MEDIA'] });
  const view = render(<SubtitlesPanel assetId="77" languages={languages} />);
  await screen.findByRole('list', { name: 'Subtitle track list' });
  return view;
}

const row = (n: number) => screen.getAllByRole('listitem')[n - 1]!;

afterEach(() => {
  cleanup();
  upload.mockReset();
});

describe('Subtitles of a video asset', () => {
  it('lists every track with its source, flags and hidden state; passes axe', async () => {
    useSubs();
    const { container } = await open();
    expect(screen.getAllByRole('listitem')).toHaveLength(4);
    expect(screen.getByLabelText('Label of track 1')).toHaveValue('English');
    expect(screen.getByLabelText('Language of track 2')).toHaveValue('es');
    expect(within(row(1)).getByText('From the file')).toBeVisible();
    expect(within(row(3)).getByText('Uploaded')).toBeVisible();
    expect(within(row(4)).getByText('Styled')).toBeVisible();
    expect(screen.getByRole('switch', { name: 'Default for English' })).toBeChecked();
    expect(screen.getByRole('switch', { name: 'Forced for Signs' })).toBeChecked();
    // A hidden track is greyed and says so; it offers Show, and only a track from the file can be hidden.
    expect(row(2)).toHaveAttribute('data-hidden', 'true');
    expect(within(row(2)).getByText('Hidden from viewers')).toBeVisible();
    expect(within(row(2)).getByRole('button', { name: 'Show Español' })).toBeVisible();
    expect(within(row(1)).getByRole('button', { name: 'Hide English' })).toBeVisible();
    expect(within(row(1)).queryByRole('button', { name: /^Delete/ })).toBeNull();
    expect(within(row(3)).getByRole('button', { name: 'Delete Deutsch' })).toBeVisible();
    expect(within(row(3)).queryByRole('button', { name: /^Hide/ })).toBeNull();
    // The sentence under the block says what a re-encode keeps.
    expect(
      screen.getByText(/Uploaded tracks are kept when the video is encoded again/),
    ).toBeVisible();
    // The title's languages are suggested in the language fields.
    const options = Array.from(container.querySelectorAll('datalist option')).map((o) =>
      o.getAttribute('value'),
    );
    expect(options).toEqual(['en', 'es', 'de']);
    expect(screen.getByLabelText('Language of track 1')).toHaveAttribute('list');
    expect(await axeViolations(container)).toEqual([]);
  });

  it('saves a label and a language when the field is left, and puts the old value back on a refusal', async () => {
    const calls = useSubs(TRACKS(), {});
    await open();
    const user = userEvent.setup();
    const label = screen.getByLabelText('Label of track 1');
    await user.clear(label);
    await user.type(label, 'English (US){Enter}');
    await waitFor(() => expect(calls.patches).toHaveLength(1));
    expect(calls.patches[0]).toEqual({ id: '1', body: { label: 'English (US)' } });
    await waitFor(() =>
      expect(screen.getByLabelText('Label of track 1')).toHaveValue('English (US)'),
    );
    const lang = screen.getByLabelText('Language of track 3');
    await user.clear(lang);
    await user.type(lang, 'pt-br');
    await user.tab();
    await waitFor(() => expect(calls.patches).toHaveLength(2));
    expect(calls.patches[1]).toEqual({ id: '3', body: { language: 'pt-br' } });
    // Leaving a field unchanged sends nothing.
    await user.click(screen.getByLabelText('Label of track 2'));
    await user.tab();
    expect(calls.patches).toHaveLength(2);
  });

  it('shows the server sentence when a language is refused and restores the field', async () => {
    const calls = useSubs(TRACKS(), { patchStatus: 400 });
    await open();
    const user = userEvent.setup();
    const lang = screen.getByLabelText('Language of track 1');
    await user.clear(lang);
    await user.type(lang, 'nope_nope');
    await user.tab();
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'language must be a BCP 47 tag such as en or pt-BR',
    );
    expect(calls.patches).toHaveLength(1);
    expect(screen.getByLabelText('Language of track 1')).toHaveValue('en');
  });

  it('each switch sends its own PATCH, and a new default clears the other rows', async () => {
    const calls = useSubs();
    await open();
    const user = userEvent.setup();
    await user.click(screen.getByRole('switch', { name: 'Forced for Deutsch' }));
    await user.click(screen.getByRole('switch', { name: 'Hearing impaired for English' }));
    await user.click(screen.getByRole('switch', { name: 'Default for Deutsch' }));
    await waitFor(() => expect(calls.patches).toHaveLength(3));
    expect(calls.patches).toEqual([
      { id: '3', body: { forced: true } },
      { id: '1', body: { hearing_impaired: true } },
      { id: '3', body: { is_default: true } },
    ]);
    await waitFor(() =>
      expect(screen.getByRole('switch', { name: 'Default for Deutsch' })).toBeChecked(),
    );
    expect(screen.getByRole('switch', { name: 'Default for English' })).not.toBeChecked();
  });

  it('hides and shows a track from the file', async () => {
    const calls = useSubs();
    await open();
    const user = userEvent.setup();
    await user.click(within(row(1)).getByRole('button', { name: 'Hide English' }));
    await waitFor(() => expect(row(1)).toHaveAttribute('data-hidden', 'true'));
    expect(calls.patches[0]).toEqual({ id: '1', body: { hidden: true } });
    await user.click(within(row(2)).getByRole('button', { name: 'Show Español' }));
    await waitFor(() => expect(row(2)).toHaveAttribute('data-hidden', 'false'));
    expect(calls.patches[1]).toEqual({ id: '2', body: { hidden: false } });
  });

  it('adds a subtitle file with its language and label, with progress, then lists it', async () => {
    useSubs();
    let release: (t: SubtitleTrack) => void = () => undefined;
    upload.mockImplementation(
      (
        _asset: string,
        _file: Blob,
        _meta: unknown,
        onProgress: (f: number) => void,
      ): Promise<SubtitleTrack> => {
        onProgress(0.4);
        return new Promise((resolve) => (release = resolve));
      },
    );
    await open();
    const user = userEvent.setup({ applyAccept: false });
    const add = screen.getByRole('button', { name: 'Add subtitles' });
    expect(add).toBeDisabled();
    await user.upload(
      screen.getByLabelText(/Subtitle file/),
      new File(['1\n00:00:01,000 --> 00:00:02,000\nHi\n'], 'movie.fr.srt'),
    );
    await user.type(screen.getByLabelText('Language of the new track'), 'fr');
    await user.type(screen.getByLabelText('Label of the new track'), 'Français');
    await user.click(add);
    expect(
      await screen.findByRole('progressbar', { name: 'Uploading the subtitle file' }),
    ).toBeVisible();
    expect(upload).toHaveBeenCalledTimes(1);
    expect(upload.mock.calls[0]![0]).toBe('77');
    expect(upload.mock.calls[0]![2]).toEqual({ language: 'fr', label: 'Français' });
    release(track({ id: '9', label: 'Français', language: 'fr', source: 'uploaded' }));
    expect(await screen.findByText('Added "Français".')).toBeInTheDocument();
    expect(screen.getByLabelText('Label of the new track')).toHaveValue('');
  });

  it("shows the server's sentence when a file is refused, and a too-large file never leaves the browser", async () => {
    useSubs();
    upload.mockRejectedValue(new ApiError(400, 'no readable cues', null));
    await open();
    const user = userEvent.setup({ applyAccept: false });
    await user.upload(
      screen.getByLabelText(/Subtitle file/),
      new File(['just words'], 'notes.srt'),
    );
    await user.click(screen.getByRole('button', { name: 'Add subtitles' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('no readable cues');
    // The storage of the video does not take uploads (409): the sentence is shown as is.
    upload.mockRejectedValue(
      new ApiError(409, "this video's storage does not take subtitle uploads", null),
    );
    await user.click(screen.getByRole('button', { name: 'Add subtitles' }));
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('does not take subtitle uploads'),
    );
    // Over 20 MiB: refused here, nothing sent.
    upload.mockClear();
    const big = new File([new Uint8Array(1)], 'big.srt');
    Object.defineProperty(big, 'size', { value: 21 * 1024 * 1024 });
    fireEvent.change(screen.getByLabelText(/Subtitle file/), { target: { files: [big] } });
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'That file is 21.0 MiB. A subtitle file can be at most 20 MiB.',
    );
    expect(screen.getByRole('button', { name: 'Add subtitles' })).toBeDisabled();
    expect(upload).not.toHaveBeenCalled();
  });

  it('deletes an uploaded track only after the confirmation', async () => {
    const calls = useSubs();
    await open();
    const user = userEvent.setup();
    await user.click(within(row(3)).getByRole('button', { name: 'Delete Deutsch' }));
    const dialog = await screen.findByRole('dialog');
    expect(dialog).toHaveTextContent('Delete the subtitles "Deutsch"?');
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(calls.deletes).toEqual([]);
    await user.click(within(row(3)).getByRole('button', { name: 'Delete Deutsch' }));
    await user.click(
      within(await screen.findByRole('dialog')).getByRole('button', { name: 'Delete subtitles' }),
    );
    await waitFor(() => expect(calls.deletes).toEqual(['3']));
    await waitFor(() => expect(screen.getAllByRole('listitem')).toHaveLength(3));
    expect(screen.queryByDisplayValue('Deutsch')).toBeNull();
    expect(screen.getByLabelText('Label of track 3')).toHaveValue('Signs');
  });

  it('says there are no subtitles, and shows a retryable error when the list cannot be read', async () => {
    useSubs([]);
    asStaff({ perms: ['STAFF_BASIC', 'PUBLISH_MEDIA'] });
    render(<SubtitlesPanel assetId="77" languages={[]} />);
    expect(await screen.findByText(/This video has no subtitles yet/)).toBeVisible();
    cleanup();
    server.use(
      http.get('*/api/v1/admin/cinema/assets/:id/subtitles', () =>
        HttpResponse.json({ code: 50001, message: 'broken' }, { status: 500 }),
      ),
    );
    render(<SubtitlesPanel assetId="77" languages={[]} />);
    expect(await screen.findByRole('alert')).toHaveTextContent('broken');
    expect(screen.getByRole('button', { name: 'Retry' })).toBeVisible();
  });
});
