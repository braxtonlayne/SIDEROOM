import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { CommandForm } from '../CommandForm';
import type { IndexedCommand } from '../commands';

const upload = vi.hoisted(() => ({ uploadFiles: vi.fn() }));
const interactions = vi.hoisted(() => ({
  sendInteraction: vi.fn(),
  requestAutocomplete: vi.fn(),
  newNonce: () => 'n-1',
}));
vi.mock('../../msg/upload', () => upload);
vi.mock('../interactions', () => interactions);
vi.mock('../useReportOutcome', () => ({ useReportOutcome: () => vi.fn() }));
vi.mock('../../data', async (orig) => ({
  ...(await orig<typeof import('../../data')>()),
  useMembers: () => [],
  useRoles: () => [],
  useChannels: () => [],
}));

const entry: IndexedCommand = {
  app: { id: '500', name: 'Scanner', icon: null, bot_id: '501' },
  command: {
    id: '10',
    application_id: '500',
    type: 1,
    name: 'scan',
    description: 'Scan a file',
    options: [{ type: 11, name: 'file', description: 'The file', required: true }],
  },
} as IndexedCommand;

describe('a command with a file option', () => {
  beforeEach(() => {
    upload.uploadFiles.mockReset();
    interactions.sendInteraction.mockReset();
    interactions.sendInteraction.mockResolvedValue({ ok: true });
  });

  it('refuses to run without the file, then uploads it and names the upload id', async () => {
    upload.uploadFiles.mockResolvedValue([{ id: 'file', uploadedFilename: '777/notes.txt' }]);
    const onClose = vi.fn();
    render(<CommandForm entry={entry} channelId="c1" guildId="g1" onClose={onClose} />);

    await userEvent.click(screen.getByRole('button', { name: 'Run command' }));
    expect(await screen.findByText('This field is required.')).toBeInTheDocument();
    expect(interactions.sendInteraction).not.toHaveBeenCalled();

    const file = new File(['hello'], 'notes.txt', { type: 'text/plain' });
    await userEvent.upload(screen.getByLabelText(/file/), file);
    expect(screen.getByText('Selected: notes.txt')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Run command' }));

    await waitFor(() => expect(interactions.sendInteraction).toHaveBeenCalled());
    expect(upload.uploadFiles.mock.calls[0]?.[0]).toBe('c1');
    expect(upload.uploadFiles.mock.calls[0]?.[1][0].file).toBe(file);
    expect(interactions.sendInteraction.mock.calls[0]?.[0]).toMatchObject({
      type: 2,
      application_id: '500',
      data: { id: '10', options: [{ type: 11, name: 'file', value: '777' }] },
    });
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it('keeps the form open and shows the reason when the upload fails', async () => {
    upload.uploadFiles.mockRejectedValue(new Error('File too large.'));
    const onClose = vi.fn();
    render(<CommandForm entry={entry} channelId="c1" guildId="g1" onClose={onClose} />);
    await userEvent.upload(screen.getByLabelText(/file/), new File(['x'], 'big.bin'));
    await userEvent.click(screen.getByRole('button', { name: 'Run command' }));
    expect(await screen.findByText('File too large.')).toBeInTheDocument();
    expect(interactions.sendInteraction).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });
});
