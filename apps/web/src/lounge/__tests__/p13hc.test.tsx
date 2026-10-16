// H-31 (D-237): application emoji in the popout and as text for other authors.
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Markdown, type MdNode, type Resolvers } from '@sideroom/markdown-react';
import { CustomEmoji, resetEmojiUrls } from '../msg/CustomEmoji';
import { toLoungeMessage } from '../liveMap';
import { resetStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';

const resolvers: Resolvers = {
  customEmoji: (e) => <CustomEmoji id={e.id} name={e.name} animated={e.animated} />,
};
const nodes: MdNode[] = [{ type: 'custom_emoji', name: 'wave', id: 77, animated: false }];

beforeEach(() => {
  resetStore();
  resetEmojiUrls();
});

describe('Application emoji (D-237)', () => {
  it('names the application in the popout and offers no join action', async () => {
    server.use(
      http.get('*/api/v1/sr/emojis/77/url', () =>
        HttpResponse.json({ url: 'https://cdn.example/e/77.png' }),
      ),
      http.get('*/api/v1/sr/emojis/77', () =>
        HttpResponse.json({
          id: '77',
          name: 'wave',
          animated: false,
          guild: null,
          application: { id: '9', name: 'Helper', icon: null },
        }),
      ),
    );
    render(<Markdown nodes={nodes} resolvers={resolvers} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Emoji :wave:' }));
    expect(await screen.findByText('Emoji of Helper')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /join/i })).not.toBeInTheDocument();
    expect(screen.queryByText('This emoji is from a private server.')).not.toBeInTheDocument();
  });

  it('carries the server hint to the view model', () => {
    const base = {
      id: '1',
      channel_id: '2',
      author: { id: '3', username: 'bob' },
      content: 'hi <:wave:77>',
      timestamp: '2026-01-01T00:00:00Z',
    };
    expect(toLoungeMessage({ ...base, sr_text_emoji: ['77'] }).textEmoji).toEqual(['77']);
    expect(toLoungeMessage(base).textEmoji).toBeUndefined();
  });
});
