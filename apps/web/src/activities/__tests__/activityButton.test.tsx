import { render, screen } from '@testing-library/react';
import { http, HttpResponse, server } from '../../test/server';
import { ActivityButton } from '../ActivityButton';

describe('ActivityButton', () => {
  it('stays hidden while Activities are off', async () => {
    server.use(
      http.get('*/api/v1/channels/7/activity', () =>
        HttpResponse.json({ enabled: false, instance: null }),
      ),
    );
    const { container } = render(<ActivityButton channelId="7" compact />);
    await new Promise((r) => setTimeout(r, 20));
    expect(container).toBeEmptyDOMElement();
  });

  it('is a small icon button in the voice panel and the larger call-bar one otherwise', async () => {
    server.use(
      http.get('*/api/v1/channels/7/activity', () =>
        HttpResponse.json({ enabled: true, instance: null }),
      ),
    );
    const { rerender } = render(<ActivityButton channelId="7" compact />);
    const small = await screen.findByRole('button', { name: /activity/i });
    expect(small).toHaveClass('lng-icon-btn');
    rerender(<ActivityButton channelId="7" />);
    expect(await screen.findByRole('button', { name: /activity/i })).toHaveClass('vc-ctl');
  });
});
