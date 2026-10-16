// H-33 / D-230: an attachment the viewer's own setting blocked has no URL and shows a note.
import { render, screen } from '@testing-library/react';
import { AttachmentList } from '../msg/Attachments';
import { toLoungeAttachment } from '../liveMap';

const wire = (over: Record<string, unknown> = {}) => ({
  id: '1',
  filename: 'cat.png',
  size: 10,
  url: '/cdn/attachments/1/1/cat.png',
  content_type: 'image/png',
  ...over,
});

describe('filtered attachments', () => {
  it('shows a note instead of the picture, keeping the name', () => {
    const a = toLoungeAttachment(wire({ url: '', sr_filtered: true }) as never, Date.now());
    expect(a.filtered).toBe(true);
    render(<AttachmentList attachments={[a]} />);
    expect(screen.getByRole('note')).toHaveTextContent('cat.png');
    expect(screen.getByRole('note')).toHaveTextContent('Hidden by your safety settings.');
    expect(document.querySelector('img')).toBeNull();
  });

  it('leaves an ordinary attachment alone', () => {
    const a = toLoungeAttachment(wire() as never, Date.now());
    expect(a.filtered).toBeUndefined();
  });
});
