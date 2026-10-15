import { act, configure, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { useStore } from '../../store';
import { emitDispatch } from '../../store/events';
import { http, HttpResponse, server } from '../../test/server';
import { useInteractionModal } from '../apps/modalStore';
import { submissionData, modalProblems } from '../apps/InteractionModal';
import { interactionConfig } from '../msg/Components';
import { at, baseHandlers, bob, calls, msg, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

// The first test also pays for loading the Lounge route chunk.
configure({ asyncUtilTimeout: 20_000 });
vi.setConfig({ testTimeout: 60_000 });

const index = {
  applications: [{ id: '500', name: 'Jukebox', icon: null, bot_id: '501' }],
  application_commands: [
    {
      id: '10',
      application_id: '500',
      type: 1,
      name: 'echo',
      description: 'Say it back',
      options: [
        { type: 3, name: 'text', description: 'What to say', required: true },
        { type: 4, name: 'times', description: 'How often', min_value: 1, max_value: 5 },
      ],
    },
    { id: '11', application_id: '500', type: 1, name: 'ping', description: 'Ping' },
  ],
};

function lastInteraction() {
  return [...calls].reverse().find((c) => c.key === 'POST /interactions');
}

describe('app interactions in the Lounge', () => {
  beforeEach(() => {
    baseHandlers();
    interactionConfig.timeoutMs = 60;
    useInteractionModal.getState().close();
    server.use(
      http.get('*/api/v1/channels/:cid/application-commands', () => HttpResponse.json(index)),
      http.post('*/api/v1/interactions', async (ctx) => {
        await record('POST')(ctx);
        return new HttpResponse(null, { status: 204 });
      }),
    );
  });

  it('presses a button as a real interaction and reports silence as a failure', async () => {
    seedLounge({
      messages: [
        msg('100', {
          content: 'approve?',
          components: [
            { type: 1, components: [{ type: 2, style: 1, label: 'Approve', custom_id: 'ok' }] },
          ],
        }),
      ],
    });
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    await userEvent.click(await within(log).findByRole('button', { name: 'Approve' }));
    await waitFor(() => expect(lastInteraction()).toBeDefined());
    const body = lastInteraction()!.body as Record<string, unknown>;
    expect(body).toMatchObject({
      type: 3,
      channel_id: 'c1',
      guild_id: 'g1',
      message_id: '100',
      data: { custom_id: 'ok', component_type: 2 },
    });
    expect(typeof body.nonce).toBe('string');
    expect(body).not.toHaveProperty('application_id');
    expect(await screen.findByText('This interaction failed')).toBeInTheDocument();
  });

  it('stops waiting as soon as the app answers', async () => {
    interactionConfig.timeoutMs = 5000;
    seedLounge({
      messages: [
        msg('100', {
          components: [
            { type: 1, components: [{ type: 2, style: 1, label: 'Go', custom_id: 'go' }] },
          ],
        }),
      ],
    });
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    const button = await within(log).findByRole('button', { name: 'Go' });
    await userEvent.click(button);
    await waitFor(() => expect(button).toHaveAttribute('aria-busy', 'true'));
    await waitFor(() => expect(lastInteraction()).toBeDefined());
    const nonce = (lastInteraction()!.body as { nonce: string }).nonce;
    act(() => emitDispatch('INTERACTION_SUCCESS', { id: '9', nonce }));
    await waitFor(() => expect(button).not.toHaveAttribute('aria-busy'));
    expect(screen.queryByText('This interaction failed')).not.toBeInTheDocument();
  });

  it('sends a select value and shows what the server said when it refuses', async () => {
    server.use(
      http.post('*/api/v1/interactions', () =>
        HttpResponse.json({ code: 10008, message: 'Unknown Message' }, { status: 404 }),
      ),
    );
    seedLounge({
      messages: [
        msg('100', {
          components: [
            {
              type: 1,
              components: [
                {
                  type: 3,
                  custom_id: 'colour',
                  placeholder: 'Pick a colour',
                  options: [{ label: 'Red', value: 'red' }],
                },
              ],
            },
          ],
        }),
      ],
    });
    at('/channels/g1/c1');
    const select = await screen.findByRole('combobox', { name: 'Pick a colour' });
    await userEvent.selectOptions(select, 'Red');
    expect(await screen.findByText('Could not use that app: Unknown Message')).toBeInTheDocument();
  });

  it('marks an ephemeral reply and dismisses it on this client only', async () => {
    seedLounge({
      messages: [
        msg('100', { content: 'public' }),
        msg('101', { content: 'secret reply', flags: 64 }),
      ],
    });
    const { container } = at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    expect(await within(log).findByText('secret reply')).toBeInTheDocument();
    expect(within(log).getAllByText('Only you can see this')).toHaveLength(1);
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(within(log).getByRole('button', { name: 'Dismiss message' }));
    await waitFor(() => expect(within(log).queryByText('secret reply')).not.toBeInTheDocument());
    expect(within(log).getByText('public')).toBeInTheDocument();
    expect(useStore.getState().messages['101']).toBeUndefined();
    expect(calls.some((c) => c.key.startsWith('DELETE'))).toBe(false);
  });

  it('offers app commands in the picker and runs one through its option form', async () => {
    seedLounge({ messages: [msg('100', { content: 'hi' })], users: { '2': bob } });
    at('/channels/g1/c1');
    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    await userEvent.type(box, '/ec');
    const option = await screen.findByRole('option', { name: /echo/ });
    expect(option).toHaveTextContent('Jukebox');
    await userEvent.click(option);
    await userEvent.type(box, 'hello world{Enter}');
    const dialog = await screen.findByRole('dialog', { name: 'Use /echo' });
    expect(within(dialog).getByLabelText(/text/)).toHaveValue('hello world');
    // an out-of-range number is refused before anything is sent
    await userEvent.type(within(dialog).getByLabelText(/times/), '9');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Run command' }));
    expect(await within(dialog).findByText('Must be 5 or less.')).toBeInTheDocument();
    expect(lastInteraction()).toBeUndefined();
    await userEvent.clear(within(dialog).getByLabelText(/times/));
    await userEvent.type(within(dialog).getByLabelText(/times/), '2');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Run command' }));
    await waitFor(() => expect(lastInteraction()).toBeDefined());
    expect(lastInteraction()!.body).toMatchObject({
      type: 2,
      application_id: '500',
      channel_id: 'c1',
      guild_id: 'g1',
      data: {
        id: '10',
        options: [
          { type: 3, name: 'text', value: 'hello world' },
          { type: 4, name: 'times', value: 2 },
        ],
      },
    });
    expect(box).toHaveValue('');
  });

  it('runs a command without options at once', async () => {
    seedLounge({ messages: [msg('100', { content: 'hi' })] });
    at('/channels/g1/c1');
    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    await screen.findByRole('log');
    // wait for the command list to arrive
    await userEvent.type(box, '/pi');
    await screen.findByRole('option', { name: /ping/ });
    await userEvent.type(box, 'ng{Enter}');
    await userEvent.type(box, '{Enter}');
    await waitFor(() => expect(lastInteraction()).toBeDefined());
    expect(lastInteraction()!.body).toMatchObject({ type: 2, data: { id: '11', options: [] } });
    expect(calls.some((c) => c.key.startsWith('POST /channels/c1/messages'))).toBe(false);
  });

  it('opens the modal an app asks for and submits its answers', async () => {
    seedLounge({ messages: [msg('100', { content: 'hi' })] });
    at('/channels/g1/c1');
    await screen.findByRole('log');
    act(() =>
      emitDispatch('INTERACTION_MODAL_CREATE', {
        id: '77',
        nonce: 'n1',
        application_id: '500',
        channel_id: 'c1',
        guild_id: 'g1',
        custom_id: 'feedback',
        title: 'Feedback',
        components: [
          {
            type: 1,
            components: [
              { type: 4, custom_id: 'subject', label: 'Subject', style: 1, max_length: 20 },
            ],
          },
          {
            type: 1,
            components: [{ type: 4, custom_id: 'body', label: 'Body', style: 2, required: false }],
          },
          {
            type: 1,
            components: [
              {
                type: 3,
                custom_id: 'mood',
                label: 'Mood',
                options: [
                  { label: 'Good', value: 'good' },
                  { label: 'Bad', value: 'bad' },
                ],
              },
            ],
          },
        ],
      }),
    );
    const dialog = await screen.findByRole('dialog', { name: 'Feedback' });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Submit' }));
    expect(await within(dialog).findAllByText('This field is required.')).toHaveLength(2);
    expect(lastInteraction()).toBeUndefined();
    await userEvent.type(within(dialog).getByLabelText(/Subject/), 'Nice app');
    await userEvent.type(within(dialog).getByLabelText(/Body/), 'More words');
    await userEvent.selectOptions(within(dialog).getByLabelText(/Mood/), 'Good');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Submit' }));
    await waitFor(() => expect(lastInteraction()).toBeDefined());
    expect(lastInteraction()!.body).toMatchObject({
      type: 5,
      application_id: '500',
      channel_id: 'c1',
      guild_id: 'g1',
      data: {
        custom_id: 'feedback',
        components: [
          { type: 1, components: [{ type: 4, custom_id: 'subject', value: 'Nice app' }] },
          { type: 1, components: [{ type: 4, custom_id: 'body', value: 'More words' }] },
          { type: 1, components: [{ type: 3, custom_id: 'mood', values: ['good'] }] },
        ],
      },
    });
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Feedback' })).toBeNull());
  });
});

describe('modal helpers', () => {
  const modal = {
    nonce: null,
    application_id: '1',
    channel_id: 'c',
    guild_id: null,
    custom_id: 'm',
    title: 'T',
    components: [
      {
        type: 1 as const,
        components: [
          { type: 4, custom_id: 'a', min_length: 3, max_length: 5 },
          { type: 4, custom_id: 'b', required: false },
        ],
      },
    ],
  };

  it('finds required, short and long values', () => {
    expect(modalProblems(modal, {})).toEqual({ a: 'required' });
    expect(modalProblems(modal, { a: 'ab' })).toEqual({ a: 'short' });
    expect(modalProblems(modal, { a: 'abcdef' })).toEqual({ a: 'long' });
    expect(modalProblems(modal, { a: 'abcd' })).toEqual({});
  });

  it('shapes the submission', () => {
    expect(submissionData(modal, { a: 'abc' })).toEqual({
      custom_id: 'm',
      components: [
        {
          type: 1,
          components: [
            { type: 4, custom_id: 'a', value: 'abc' },
            { type: 4, custom_id: 'b', value: '' },
          ],
        },
      ],
    });
  });
});
