import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RoomImage, clearImageLinks, mountImageUrl } from '../tierlist/RoomImage';

const presignGet = vi.fn();
vi.mock('../../storage/api', () => ({ presignGet: (...a: unknown[]) => presignGet(...a) }));

beforeEach(() => {
  presignGet.mockReset();
  clearImageLinks();
});

const future = () => new Date(Date.now() + 10 * 60_000).toISOString();

describe('room images are presigned per viewer (UTL-COL-001)', () => {
  it('asks the storage presign endpoint for the viewer, not the room, and shows the picture', async () => {
    presignGet.mockResolvedValue({
      url: 'https://bucket.example/a.png?sig=1',
      expires_at: future(),
    });
    render(<RoomImage img={{ k: 'mount', mount: '55', key: 'pics/a.png' }} label="a" />);
    const img = await screen.findByRole('img', { name: 'a' });
    expect(img).toHaveAttribute('src', 'https://bucket.example/a.png?sig=1');
    expect(presignGet).toHaveBeenCalledWith('55', 'pics/a.png');
  });

  it('shows a placeholder when the viewer has no access to the mount', async () => {
    presignGet.mockRejectedValue(new Error('403'));
    render(<RoomImage img={{ k: 'mount', mount: '55', key: 'private.png' }} label="p" />);
    expect(await screen.findByRole('img', { name: /do not have access/ })).toBeInTheDocument();
    expect(document.querySelector('img')).toBeNull();
  });

  it('reuses a link until shortly before it expires, then asks again', async () => {
    presignGet.mockResolvedValue({ url: 'https://b/x?1', expires_at: future() });
    await mountImageUrl('1', 'k');
    await mountImageUrl('1', 'k');
    expect(presignGet).toHaveBeenCalledTimes(1);
    presignGet.mockResolvedValue({
      url: 'https://b/x?2',
      expires_at: new Date(Date.now() + 1000).toISOString(),
    });
    clearImageLinks();
    expect(await mountImageUrl('1', 'k')).toBe('https://b/x?2');
    // Within the 30 s skew of its expiry the link counts as stale.
    await waitFor(async () => {
      await mountImageUrl('1', 'k');
      expect(presignGet).toHaveBeenCalledTimes(3);
    });
  });

  it('serves a same-origin path as it is, without any presign', async () => {
    render(<RoomImage img={{ k: 'path', path: '/api/v1/attachments/1/2/a.png' }} label="att" />);
    expect(await screen.findByRole('img', { name: 'att' })).toHaveAttribute(
      'src',
      '/api/v1/attachments/1/2/a.png',
    );
    expect(presignGet).not.toHaveBeenCalled();
  });
});
