import { render, screen } from '@testing-library/react';
import { UploadTray } from '../msg/UploadTray';
import type { PendingFile } from '../msg/upload';
import { resetUploadLimits } from '../msg/uploadLimit';
import { http, HttpResponse, server } from '../../test/server';

const file = (name: string, size: number): PendingFile => {
  const f = new File(['x'], name, { type: 'video/mp4' });
  Object.defineProperty(f, 'size', { value: size });
  return {
    id: name,
    file: f,
    name,
    spoiler: false,
    description: '',
    progress: 0,
    status: 'queued',
  };
};

const me = (patron: boolean) =>
  http.get('*/api/v1/donations/me', () =>
    HttpResponse.json({
      patron: { active: patron, in_grace: false, permanent: false },
      limits: { upload_bytes: 10 * 1024 * 1024, patron },
      preferences: {},
    }),
  );

beforeEach(() => resetUploadLimits());

describe('Upload tray size check (LNG-CMP-005)', () => {
  it('warns about a file over the limit and mentions Patron for free members', async () => {
    server.use(me(false));
    render(
      <UploadTray
        files={[file('big.mp4', 50 * 1024 * 1024), file('ok.mp4', 9 * 1024 * 1024)]}
        busy={false}
        onChange={() => undefined}
        onRemove={() => undefined}
      />,
    );
    expect(await screen.findByText(/Patrons can upload larger files/)).toBeInTheDocument();
    expect(screen.getAllByText(/larger than your/)).toHaveLength(1);
  });

  it('does not suggest Patron to a Patron and skips the lookup for small files', async () => {
    server.use(me(true));
    const { unmount } = render(
      <UploadTray
        files={[file('big.mp4', 50 * 1024 * 1024)]}
        busy={false}
        onChange={() => undefined}
        onRemove={() => undefined}
      />,
    );
    expect(await screen.findByText(/larger than your 10.0 MB upload limit/)).toBeInTheDocument();
    expect(screen.queryByText(/Patrons can upload/)).toBeNull();
    unmount();
    let asked = false;
    server.use(
      http.get('*/api/v1/donations/me', () => {
        asked = true;
        return HttpResponse.json({});
      }),
    );
    resetUploadLimits();
    render(
      <UploadTray
        files={[file('small.mp4', 1024)]}
        busy={false}
        onChange={() => undefined}
        onRemove={() => undefined}
      />,
    );
    expect(asked).toBe(false);
  });
});
