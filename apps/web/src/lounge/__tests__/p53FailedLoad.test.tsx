// SEC-AUD-014 (wave 53): a failed load shows an error with Retry and keeps Save off; a 404 stays an empty form;
// a failed save never claims "Saved".
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { BirthdayCard } from '../../settings/user/sections/BirthdayCard';
import { http, HttpResponse, server } from '../../test/server';
import { NoteField } from '../profile/parts';
import { GuildProfileDialog } from '../shell/GuildProfileDialog';
import { NoteDialog } from '../shell/UserDialogs';
import { baseHandlers, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const fail500 = () => HttpResponse.json({ message: 'boom' }, { status: 500 });
const notFound = () => HttpResponse.json({ message: 'none' }, { status: 404 });

beforeEach(() => {
  baseHandlers();
  seedLounge({ messages: [] });
});

describe('the per-server profile dialog', () => {
  it('a failed read locks the form, offers Retry, and Retry loads it', async () => {
    let fails = true;
    const patches: unknown[] = [];
    server.use(
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        fails ? fail500() : HttpResponse.json({ nick: 'Old nick', bio: 'old bio', roles: [] }),
      ),
      http.patch('*/api/v1/guilds/g1/members/@me', async ({ request }) => {
        patches.push(await request.json());
        return HttpResponse.json({ nick: 'x', roles: [] });
      }),
    );
    render(<GuildProfileDialog guildId="g1" onClose={() => undefined} />);
    expect(await screen.findByText(/could not be loaded, so saving is off/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(screen.getByLabelText('Nickname')).toBeDisabled();
    fails = false;
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(screen.getByLabelText('Nickname')).toHaveValue('Old nick'));
    expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
    expect(screen.queryByText(/saving is off/)).toBeNull();
    expect(patches).toEqual([]);
  });

  it('a 404 means nothing set yet: an empty, editable form', async () => {
    server.use(http.get('*/api/v1/users/@me/guilds/g1/member', notFound));
    render(<GuildProfileDialog guildId="g1" onClose={() => undefined} />);
    await waitFor(() => expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled());
    expect(screen.queryByText(/saving is off/)).toBeNull();
  });
});

describe('the note dialog', () => {
  it('a failed read shows an error and Retry instead of an empty note form', async () => {
    let fails = true;
    server.use(
      http.get('*/api/v1/users/@me/notes/2', () =>
        fails ? fail500() : HttpResponse.json({ note: 'met at the meetup' }),
      ),
    );
    render(<NoteDialog userId="2" name="Bob" onClose={() => undefined} />);
    expect(await screen.findByText(/could not be loaded, so saving is off/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Save' })).toBeNull();
    fails = false;
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByDisplayValue('met at the meetup')).toBeInTheDocument();
  });

  it('a 404 is a fresh empty note that can be saved', async () => {
    server.use(http.get('*/api/v1/users/@me/notes/2', notFound));
    render(<NoteDialog userId="2" name="Bob" onClose={() => undefined} />);
    expect(await screen.findByRole('button', { name: 'Save' })).toBeEnabled();
  });
});

describe('the note box in the profile pop-out', () => {
  it('a failed read locks the box so a blur cannot erase the note', async () => {
    const puts: unknown[] = [];
    server.use(
      http.get('*/api/v1/users/@me/notes/2', fail500),
      http.put('*/api/v1/users/@me/notes/2', async ({ request }) => {
        puts.push(await request.json());
        return new HttpResponse(null, { status: 204 });
      }),
    );
    render(<NoteField userId="2" />);
    expect(await screen.findByText(/could not be loaded/)).toBeInTheDocument();
    const box = screen.getByRole('textbox');
    expect(box).toBeDisabled();
    box.focus();
    box.blur();
    expect(puts).toEqual([]);
  });

  it('a failed save says so, and the next blur tries again', async () => {
    let ok = false;
    const puts: unknown[] = [];
    server.use(
      http.get('*/api/v1/users/@me/notes/2', notFound),
      http.put('*/api/v1/users/@me/notes/2', async ({ request }) => {
        puts.push(await request.json());
        return ok ? new HttpResponse(null, { status: 204 }) : fail500();
      }),
    );
    render(<NoteField userId="2" />);
    const box = await screen.findByRole('textbox');
    await userEvent.type(box, 'hello');
    await userEvent.tab();
    expect(await screen.findByText('Your note was not saved.')).toBeInTheDocument();
    ok = true;
    await userEvent.click(box);
    await userEvent.tab();
    await waitFor(() => expect(puts).toHaveLength(2));
    await waitFor(() => expect(screen.queryByText('Your note was not saved.')).toBeNull());
  });
});

describe('the birthday card', () => {
  it('a failed read shows the error with Retry and no form to save over it', async () => {
    let fails = true;
    server.use(
      http.get('*/api/v1/users/@me/birthday', () =>
        fails
          ? fail500()
          : HttpResponse.json({ month: 12, day: 25, year: 1990, visibility: 'servers' }),
      ),
    );
    render(<BirthdayCard />);
    expect(await screen.findByText('Could not load your birthday.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Save birthday' })).toBeNull();
    fails = false;
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('combobox', { name: 'Month' })).toHaveTextContent('December');
  });

  it('a 404 stays an empty form', async () => {
    server.use(http.get('*/api/v1/users/@me/birthday', notFound));
    render(<BirthdayCard />);
    expect(await screen.findByRole('button', { name: 'Save birthday' })).toBeEnabled();
  });
});
