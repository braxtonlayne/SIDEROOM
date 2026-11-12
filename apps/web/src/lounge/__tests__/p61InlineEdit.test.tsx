// SEC-AUD-135 (wave 61): a failed inline edit keeps the editor open with what was typed.
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { warmLoungeRoute } from '../../test/warmRoutes';
import { at, baseHandlers, me, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

warmLoungeRoute();

beforeEach(() => baseHandlers());

async function openEditor() {
  seedLounge({ messages: [msg('100', { author: me, content: 'first words' })] });
  at('/channels/g1/c1');
  const box = await screen.findByRole('textbox', { name: 'Message #general' });
  await userEvent.click(box);
  await userEvent.keyboard('{ArrowUp}');
  const editor = await screen.findByRole('textbox', { name: 'Edit message' });
  await userEvent.clear(editor);
  await userEvent.type(editor, 'second words');
  return editor;
}

describe('inline edit result (SEC-AUD-135)', () => {
  it('a_failed_edit_keeps_the_editor_open_with_the_text', async () => {
    server.use(
      http.patch('*/api/v1/channels/c1/messages/100', () =>
        HttpResponse.json({ message: 'boom' }, { status: 500 }),
      ),
    );
    const editor = await openEditor();
    await userEvent.keyboard('{Enter}');
    expect(await screen.findByText('Your edit was not saved. Try again.')).toBeInTheDocument();
    const still = screen.getByRole('textbox', { name: 'Edit message' });
    expect(still).toBe(editor);
    expect(still).toHaveValue('second words');
    expect(still).toBeEnabled();
  });

  it('a_saved_edit_closes_the_editor', async () => {
    server.use(
      http.patch('*/api/v1/channels/c1/messages/100', () =>
        HttpResponse.json(msg('100', { author: me, content: 'second words' })),
      ),
    );
    await openEditor();
    await userEvent.keyboard('{Enter}');
    await waitFor(() => expect(screen.queryByRole('textbox', { name: 'Edit message' })).toBeNull());
    expect(await screen.findByText('second words')).toBeInTheDocument();
  });

  it('the_editor_is_disabled_while_saving', async () => {
    let release!: () => void;
    let patches = 0;
    server.use(
      http.patch('*/api/v1/channels/c1/messages/100', async () => {
        patches += 1;
        await new Promise<void>((r) => (release = r));
        return HttpResponse.json(msg('100', { author: me, content: 'second words' }));
      }),
    );
    const editor = await openEditor();
    await userEvent.keyboard('{Enter}');
    await waitFor(() => expect(editor).toBeDisabled());
    await userEvent.click(screen.getByRole('button', { name: 'save' }));
    release();
    await waitFor(() => expect(screen.queryByRole('textbox', { name: 'Edit message' })).toBeNull());
    expect(patches).toBe(1);
  });
});
