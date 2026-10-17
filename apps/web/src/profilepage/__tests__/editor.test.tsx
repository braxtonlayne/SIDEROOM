import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { resetStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { signedIn } from '../../settings/user/__tests__/helpers';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { cleanDoc } from '../ProfileEditor';
import { contrast, lowContrast } from '../StylePanel';
import { emptyDoc, type ProfileDoc } from '../types';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const ME_PAGE = (doc: ProfileDoc) => ({
  user: {
    id: '1',
    username: 'alice',
    display_name: 'Alice',
    avatar: null,
    banner: null,
    accent_color: null,
  },
  doc,
  friends: [],
  is_owner: true,
  can_sign: true,
  customised: doc.blocks.length > 0,
});

interface Rig {
  saved: ProfileDoc[];
  /** The uploads the server was sent: the kind and the size of the body. */
  uploads: { kind: string; bytes: number; type: string }[];
  deleted: string[];
  media: Record<string, unknown>[];
}

function open(initial: ProfileDoc = emptyDoc(), extra: Parameters<typeof server.use> = []): Rig {
  signedIn();
  const rig: Rig = { saved: [], uploads: [], deleted: [], media: [] };
  server.use(
    ...extra,
    http.get('*/api/v1/users/@me/profile-doc', () => HttpResponse.json(initial)),
    http.get('*/api/v1/users/@me/profile-page', () => HttpResponse.json(ME_PAGE(initial))),
    http.get('*/api/v1/users/@me/relationships', () =>
      HttpResponse.json([
        {
          id: '3',
          type: 1,
          since: '2026-01-01',
          user: { id: '3', username: 'cy', global_name: 'Cy' },
        },
        { id: '4', type: 2, since: '2026-01-01', user: { id: '4', username: 'blocked' } },
      ]),
    ),
    http.get('*/api/v1/packages', () => HttpResponse.json([])),
    http.get('*/api/v1/users/@me/profile-media', () => HttpResponse.json(rig.media)),
    http.post('*/api/v1/users/@me/profile-media', async ({ request }) => {
      const kind = new URL(request.url).searchParams.get('kind') ?? '';
      const body = await request.arrayBuffer();
      rig.uploads.push({ kind, bytes: body.byteLength, type: request.headers.get('content-type') ?? '' });
      const id = String(100 + rig.uploads.length);
      const m = {
        id,
        kind,
        url: `/cdn/avatars/1/${id}.png`,
        content_type: 'image/png',
        width: 64,
        height: 48,
        bytes: body.byteLength,
        created_at_ms: 1,
      };
      rig.media.push(m);
      return HttpResponse.json(m, { status: 201 });
    }),
    http.delete('*/api/v1/users/@me/profile-media/:id', ({ params }) => {
      rig.deleted.push(String(params.id));
      rig.media = rig.media.filter((m) => m.id !== params.id);
      return new HttpResponse(null, { status: 204 });
    }),
    http.put('*/api/v1/users/@me/profile-doc', async ({ request }) => {
      const body = (await request.json()) as ProfileDoc;
      rig.saved.push(body);
      return HttpResponse.json(body);
    }),
  );
  resetStore();
  const router = createAppRouter(createMemoryHistory({ initialEntries: ['/profile-page/edit'] }));
  render(<RouterProvider router={router} />);
  return rig;
}

describe('profile editor (EXT-PRF-005)', () => {
  it('adds blocks on the grid, edits one and saves the document', async () => {
    const rig = open();
    const user = userEvent.setup();
    const canvas = await screen.findByRole('group', { name: 'Profile page grid' });
    expect(screen.getByRole('button', { name: 'Save page' })).toBeDisabled();

    await user.click(screen.getByRole('button', { name: 'Add block' }));
    // The block type defaults to the first one that can be added: a header.
    const header = within(canvas).getByRole('button', { name: /Header block at column 1, row 1/ });
    expect(header).toHaveAttribute('aria-pressed', 'true');

    await user.type(screen.getByLabelText('Tagline'), 'hello');
    await user.click(screen.getByRole('combobox', { name: 'Block type' }));
    await user.click(await screen.findByRole('option', { name: 'About me' }));
    await user.click(screen.getByRole('button', { name: 'Add block' }));
    const about = within(canvas).getByRole('button', { name: /About me block at column 1, row 4/ });
    expect(about).toBeInTheDocument();
    await user.type(screen.getByLabelText('About me'), 'I make soup');

    await user.click(screen.getByRole('button', { name: 'Save page' }));
    await waitFor(() => expect(rig.saved).toHaveLength(1));
    const doc = rig.saved[0]!;
    expect(doc.blocks.map((b) => [b.id, b.type, b.x, b.y, b.w, b.h])).toEqual([
      ['header-1', 'header', 0, 0, 12, 3],
      ['about-1', 'about', 0, 3, 6, 4],
    ]);
    expect(doc.blocks[0]!.config).toEqual({ tagline: 'hello' });
    expect(doc.blocks[1]!.config).toEqual({ text: 'I make soup' });
    expect(await screen.findByText('Your page is live.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save page' })).toBeDisabled();
  });

  it('moves and resizes the selected block from the keyboard, refusing overlaps', async () => {
    const start: ProfileDoc = {
      ...emptyDoc(),
      blocks: [
        { id: 'a', type: 'about', x: 0, y: 0, w: 6, h: 3, config: {} },
        { id: 'c', type: 'custom', x: 6, y: 0, w: 6, h: 3, config: {} },
      ],
    };
    const rig = open(start);
    const user = userEvent.setup();
    const tile = await screen.findByRole('button', { name: /About me block at column 1, row 1/ });
    tile.focus();
    // Right would run into the Custom box: refused.
    fireEvent.keyDown(tile, { key: 'ArrowRight' });
    expect(tile).toHaveAccessibleName(/column 1, row 1/);
    fireEvent.keyDown(tile, { key: 'ArrowDown' });
    expect(tile).toHaveAccessibleName(/column 1, row 2/);
    fireEvent.keyDown(tile, { key: 'ArrowDown', shiftKey: true });
    expect(tile).toHaveAccessibleName(/6 columns by 4 rows/);
    // Shrinking works, and so does moving onto the row the other block does not use.
    fireEvent.keyDown(tile, { key: 'ArrowLeft', shiftKey: true });
    expect(tile).toHaveAccessibleName(/5 columns by 4 rows/);

    await user.click(screen.getByRole('button', { name: 'Save page' }));
    await waitFor(() => expect(rig.saved).toHaveLength(1));
    expect(rig.saved[0]!.blocks[0]).toMatchObject({ x: 0, y: 1, w: 5, h: 4 });
    expect(rig.saved[0]!.blocks[1]).toMatchObject({ x: 6, y: 0, w: 6, h: 3 });
  });

  it('uploads a background picture and a gallery picture and saves the document that names them', async () => {
    const start: ProfileDoc = {
      ...emptyDoc(),
      blocks: [{ id: 'g', type: 'gallery', x: 0, y: 0, w: 6, h: 3, config: {} }],
    };
    const rig = open(start);
    const user = userEvent.setup();
    const png = new File([new Uint8Array([137, 80, 78, 71, 1, 2, 3])], 'bg.png', { type: 'image/png' });

    // The background picture is the request body, kind=background, and becomes the page background.
    await screen.findByRole('group', { name: 'Profile page grid' });
    await user.upload(screen.getByLabelText('Background picture file'), png);
    await waitFor(() => expect(rig.uploads).toHaveLength(1));
    expect(rig.uploads[0]).toMatchObject({ kind: 'background', bytes: 7, type: 'image/png' });
    expect(await screen.findByRole('button', { name: 'Remove the background picture' })).toBeInTheDocument();

    // A gallery picture is added to the selected gallery block.
    await user.click(screen.getByRole('button', { name: /Gallery block/ }));
    await user.upload(screen.getByLabelText('Gallery picture file'), png);
    await waitFor(() => expect(rig.uploads).toHaveLength(2));
    expect(rig.uploads[1]).toMatchObject({ kind: 'gallery' });

    await user.click(screen.getByRole('button', { name: 'Save page' }));
    await waitFor(() => expect(rig.saved).toHaveLength(1));
    const saved = rig.saved[0]!;
    expect(saved.style.background).toEqual({ kind: 'image', pattern: 'none', image_id: '101' });
    expect(saved.style.banner_background).toBe(false);
    expect(saved.blocks[0]!.config).toEqual({ items: [{ image_id: '102' }] });
  });

  it('removing an uploaded picture deletes it on the server and leaves the document without it', async () => {
    const start: ProfileDoc = {
      ...emptyDoc(),
      style: { ...emptyDoc().style, background: { kind: 'image', pattern: 'none', image_id: '11' } },
      blocks: [
        {
          id: 'g',
          type: 'gallery',
          x: 0,
          y: 0,
          w: 6,
          h: 3,
          config: { items: [{ image_id: '21', caption: 'Mine' }] },
        },
      ],
    };
    const rig = open(start, [
      http.get('*/api/v1/users/@me/profile-media', () =>
        HttpResponse.json([
          { id: '11', kind: 'background', url: '/cdn/avatars/1/11.png', content_type: 'image/png', width: 1, height: 1, bytes: 1, created_at_ms: 1 },
          { id: '21', kind: 'gallery', url: '/cdn/avatars/1/21.png', content_type: 'image/png', width: 1, height: 1, bytes: 1, created_at_ms: 1 },
        ]),
      ),
    ]);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /Gallery block/ }));
    await user.click(screen.getByRole('button', { name: 'Remove picture' }));
    expect(rig.deleted).toEqual(['21']);
    await user.click(await screen.findByRole('button', { name: 'Remove the background picture' }));
    expect(rig.deleted).toEqual(['21', '11']);
    await user.click(screen.getByRole('button', { name: 'Save page' }));
    await waitFor(() => expect(rig.saved).toHaveLength(1));
    expect(rig.saved[0]!.style.background).toEqual({ kind: 'pattern', pattern: 'none' });
    expect(rig.saved[0]!.blocks[0]!.config).toEqual({ items: [] });
  });

  it('has no accessibility violations', async () => {
    open({
      ...emptyDoc(),
      blocks: [
        { id: 'a', type: 'about', x: 0, y: 0, w: 6, h: 3, config: {} },
        { id: 'c', type: 'custom', x: 6, y: 0, w: 6, h: 3, config: {} },
      ],
    });
    const canvas = await screen.findByRole('group', { name: 'Profile page grid' });
    await userEvent.setup().click(within(canvas).getByRole('button', { name: /About me block/ }));
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('removes a block with Delete and Revert brings the saved page back', async () => {
    const start: ProfileDoc = {
      ...emptyDoc(),
      blocks: [{ id: 'a', type: 'about', x: 0, y: 0, w: 6, h: 3, config: { text: 'x' } }],
    };
    open(start);
    const user = userEvent.setup();
    const tile = await screen.findByRole('button', { name: /About me block/ });
    tile.focus();
    fireEvent.keyDown(tile, { key: 'Delete' });
    expect(screen.queryByRole('button', { name: /About me block/ })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Revert' }));
    expect(await screen.findByRole('button', { name: /About me block/ })).toBeInTheDocument();
  });

  it('lists every problem the server finds and keeps the draft', async () => {
    open(emptyDoc(), [
      http.put('*/api/v1/users/@me/profile-doc', () =>
        HttpResponse.json(
          {
            code: 50035,
            message: 'Invalid Form Body',
            errors: {
              problems: {
                '0': {
                  _errors: [
                    {
                      code: 'INVALID_VALUE',
                      message: 'blocks[0].config.url: must be an https link',
                    },
                  ],
                },
                '1': {
                  _errors: [
                    { code: 'INVALID_VALUE', message: 'style.palette.text: contrast 2.1:1' },
                  ],
                },
              },
            },
          },
          { status: 400 },
        ),
      ),
    ]);
    const user = userEvent.setup();
    await screen.findByRole('group', { name: 'Profile page grid' });
    await user.click(screen.getByRole('button', { name: 'Add block' }));
    await user.click(screen.getByRole('button', { name: 'Save page' }));
    const alert = await screen.findByText('Fix these first');
    expect(alert).toBeInTheDocument();
    expect(screen.getByText('blocks[0].config.url: must be an https link')).toBeInTheDocument();
    expect(screen.getByText('style.palette.text: contrast 2.1:1')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Header block/ })).toBeInTheDocument();
  });

  it('offers only friends for Top friends, up to eight', async () => {
    open({
      ...emptyDoc(),
      blocks: [{ id: 'f', type: 'top_friends', x: 0, y: 0, w: 6, h: 4, config: {} }],
    });
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /Top friends block/ }));
    const legend = await screen.findByText('Choose up to 8 friends');
    const box = await screen.findByRole('checkbox', { name: 'Cy' });
    expect(legend).toBeInTheDocument();
    expect(screen.queryByRole('checkbox', { name: 'blocked' })).toBeNull();
    await user.click(box);
    expect(box).toBeChecked();
  });

  it('shows the real renderer in the preview tab', async () => {
    open({
      ...emptyDoc(),
      blocks: [{ id: 'a', type: 'about', x: 0, y: 0, w: 6, h: 3, config: { text: 'Preview me' } }],
    });
    const user = userEvent.setup();
    await screen.findByRole('group', { name: 'Profile page grid' });
    await user.click(screen.getByRole('radio', { name: 'Preview' }));
    expect(await screen.findByText('Preview me')).toBeInTheDocument();
    expect(document.querySelector('.pp-root')).not.toBeNull();
  });

  it('warns when the text colour is hard to read', async () => {
    open();
    const user = userEvent.setup();
    await screen.findByRole('group', { name: 'Profile page grid' });
    const hex = await screen.findByLabelText('Text colour as #rrggbb');
    expect(screen.queryByRole('alert')).toBeNull();
    await user.clear(hex);
    await user.type(hex, '#222222');
    expect(await screen.findByText(/hard to read/)).toBeInTheDocument();
  });

  it('starts from an approved layout', async () => {
    const used: string[] = [];
    const layoutDoc: ProfileDoc = {
      ...emptyDoc(),
      layout: 'retro',
      blocks: [{ id: 'a', type: 'about', x: 0, y: 0, w: 12, h: 4, config: {} }],
    };
    open(emptyDoc(), [
      http.get('*/api/v1/packages', () =>
        HttpResponse.json([
          {
            id: '55',
            kind: 'profile_layout',
            slug: 'retro',
            name: 'Retro',
            description: '',
            author_id: '9',
            author_name: 'Ada',
            version_id: '56',
            version: '1.0.0',
            size_bytes: 10,
            updated_at_ms: 0,
            payload: { doc: layoutDoc },
          },
        ]),
      ),
      http.post('*/api/v1/users/@me/profile-doc/layout', async ({ request }) => {
        used.push(((await request.json()) as { package_id: string }).package_id);
        return HttpResponse.json(layoutDoc);
      }),
    ]);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('combobox', { name: 'Layout' }));
    await user.click(await screen.findByRole('option', { name: 'Retro (Ada)' }));
    await user.click(screen.getByRole('button', { name: 'Use this layout' }));
    await waitFor(() => expect(used).toEqual(['55']));
    expect(
      await screen.findByRole('button', { name: /About me block at column 1, row 1/ }),
    ).toBeInTheDocument();
  });
});

describe('editor helpers', () => {
  it('drops empty favourites before saving', () => {
    const d = cleanDoc({
      ...emptyDoc(),
      blocks: [
        {
          id: 'f',
          type: 'favorites',
          x: 0,
          y: 0,
          w: 6,
          h: 4,
          config: { items: ['miso', '  ', ''] },
        },
      ],
    });
    expect(d.blocks[0]!.config.items).toEqual(['miso']);
  });

  it('measures contrast like the server does', () => {
    expect(contrast('#000000', '#ffffff')).toBeCloseTo(21, 1);
    expect(contrast('red', '#ffffff')).toBeNull();
    expect(
      lowContrast({ background: '#ffffff', panel: '#000000', text: '#eeeeee', accent: '#ff0000' }),
    ).toEqual(['background']);
  });
});
