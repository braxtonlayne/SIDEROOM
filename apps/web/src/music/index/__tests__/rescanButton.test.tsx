import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { RescanTagsButton } from '../RescanTagsButton';
import { Tagger } from '../tagger';
import type { TaggerApi } from '../types';

function api(): TaggerApi {
  return {
    listPending: vi.fn(async () => ({ items: [], total: 0 })),
    presign: vi.fn(),
    patch: vi.fn(),
    rescan: vi.fn(async () => ({ queued: 3 })),
    uploadCover: vi.fn(),
  } as unknown as TaggerApi;
}

describe('RescanTagsButton (MUS-IDX-001)', () => {
  it('asks the server to queue the mount, then reads', async () => {
    const a = api();
    const tagger = new Tagger({
      api: a,
      reader: async () => ({ readTags: vi.fn(), readTagsTail: vi.fn() }),
    });
    render(<RescanTagsButton mountId="10" tagger={tagger} />);
    await userEvent.click(screen.getByRole('button', { name: 'Rescan tags' }));
    await waitFor(() => expect(a.rescan).toHaveBeenCalledWith('10'));
    await waitFor(() => expect(a.listPending).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByRole('button', { name: 'Rescan tags' })).toBeEnabled());
  });

  it('has no accessibility violations', async () => {
    const tagger = new Tagger({
      api: api(),
      reader: async () => ({ readTags: vi.fn(), readTagsTail: vi.fn() }),
    });
    render(<RescanTagsButton mountId="10" tagger={tagger} />);
    expect(await axeViolations(document.body)).toEqual([]);
  });
});
