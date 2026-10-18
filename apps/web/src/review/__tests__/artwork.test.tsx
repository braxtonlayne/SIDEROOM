import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { ArtworkField, SLOT_SHAPE } from '../ArtworkField';

// The crop dialog draws on a canvas, which jsdom does not have: stand in with a dialog that "crops" at once.
vi.mock('../../settings/user/ImageCropDialog', () => ({
  ImageCropDialog: ({
    open,
    title,
    aspect,
    onApply,
  }: {
    open: boolean;
    title: string;
    aspect: number;
    onApply: (uri: string) => void;
  }) =>
    open ? (
      <div role="dialog" aria-label={title} data-aspect={aspect}>
        <button type="button" onClick={() => onApply('data:image/png;base64,AAAA')}>
          Use crop
        </button>
      </div>
    ) : null,
}));

describe('artwork field (REV-ED-003, K-11)', () => {
  beforeEach(() => {
    server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })));
  });

  it('crops the picked image to the slot and sends it to the artwork route', async () => {
    const user = userEvent.setup();
    let body: unknown;
    server.use(
      http.post('*/api/v1/submissions/5/artwork/cover', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ kind: 'cover', url: '/cdn/library/9/cover-1.png' });
      }),
    );
    const saved = vi.fn();
    render(<ArtworkField submissionId="5" slot="cover" onSaved={saved} />);
    await user.upload(
      screen.getByLabelText('cover image file'),
      new File([new Uint8Array(10)], 'c.png', { type: 'image/png' }),
    );
    const dialog = await screen.findByRole('dialog', { name: 'Crop the cover' });
    expect(Number(dialog.getAttribute('data-aspect'))).toBeCloseTo(SLOT_SHAPE.cover.aspect);
    await user.click(screen.getByRole('button', { name: 'Use crop' }));
    await waitFor(() => expect(body).toEqual({ image: 'data:image/png;base64,AAAA' }));
    expect(await screen.findByText('Saved.')).toBeInTheDocument();
    expect(saved).toHaveBeenCalled();
  });

  it('refuses a file that is not an image before any crop', async () => {
    const user = userEvent.setup({ applyAccept: false });
    render(<ArtworkField submissionId="5" slot="backdrop" />);
    await user.upload(
      screen.getByLabelText('backdrop image file'),
      new File(['x'], 'a.txt', { type: 'text/plain' }),
    );
    expect(await screen.findByText(/Choose a PNG, JPEG, GIF or WebP image/)).toBeInTheDocument();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('shows the server refusal (for example a submission that is no longer editable)', async () => {
    const user = userEvent.setup();
    server.use(
      http.post('*/api/v1/submissions/5/artwork/poster', () =>
        HttpResponse.json(
          { code: 900001, message: 'a submitted submission cannot be edited' },
          { status: 409 },
        ),
      ),
    );
    render(<ArtworkField submissionId="5" slot="poster" />);
    await user.upload(
      screen.getByLabelText('poster image file'),
      new File([new Uint8Array(10)], 'p.png', { type: 'image/png' }),
    );
    await user.click(await screen.findByRole('button', { name: 'Use crop' }));
    expect(await screen.findByText(/cannot be edited/)).toBeInTheDocument();
  });
});
