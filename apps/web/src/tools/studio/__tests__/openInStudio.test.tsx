import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Attachment } from '../../../lounge/data';
import { Lightbox } from '../../../lounge/msg/Attachments';
import { installFakeCanvas } from './fakeCanvas';

const att = (filename: string, contentType: string): Attachment => ({
  id: filename,
  filename,
  size: 10,
  contentType,
  url: `/files/${filename}`,
});

beforeEach(() => installFakeCanvas());
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('Open in studio (UTL-IMG-001)', () => {
  it('is offered for a still image, not for a GIF', () => {
    const { unmount } = render(
      <Lightbox
        items={[att('a.png', 'image/png')]}
        index={0}
        onIndex={() => {}}
        onClose={() => {}}
      />,
    );
    expect(screen.getByTestId('open-in-studio')).toBeInTheDocument();
    unmount();
    render(
      <Lightbox
        items={[att('a.gif', 'image/gif')]}
        index={0}
        onIndex={() => {}}
        onClose={() => {}}
      />,
    );
    expect(screen.queryByTestId('open-in-studio')).not.toBeInTheDocument();
  });

  it('copies the image into the studio and never writes to the original', async () => {
    const calls: { url: string; method: string }[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string, init?: RequestInit) => {
        calls.push({ url, method: init?.method ?? 'GET' });
        return new Response(new Blob(['x'], { type: 'image/png' }));
      }),
    );
    vi.stubGlobal(
      'createImageBitmap',
      vi.fn(async () => ({ width: 8, height: 4, close() {} })),
    );
    render(
      <Lightbox
        items={[att('a.png', 'image/png')]}
        index={0}
        onIndex={() => {}}
        onClose={() => {}}
      />,
    );
    await userEvent.click(screen.getByTestId('open-in-studio'));
    expect(
      await screen.findByText('The studio works on a copy. The original message is not changed.'),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole('img', { name: 'Image preview, 8 by 4 pixels' })).toBeInTheDocument(),
    );
    expect(calls).toEqual([{ url: '/files/a.png', method: 'GET' }]);
    await userEvent.click(screen.getByRole('button', { name: 'Export' }));
    expect(screen.getByRole('button', { name: 'Save as a copy' })).toBeInTheDocument();
  });
});
