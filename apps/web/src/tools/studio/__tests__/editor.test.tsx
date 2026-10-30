import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { UploadTray } from '../../../lounge/msg/UploadTray';
import type { PendingFile } from '../../../lounge/msg/upload';
import { createBitmap } from '../bitmap';
import { StudioEditor } from '../StudioEditor';
import { bytesOf, containsBytes, installFakeCanvas } from './fakeCanvas';

// jsdom has no PointerEvent: a MouseEvent carries the coordinates the editor reads.
beforeEach(() => {
  installFakeCanvas();
  if (typeof window.PointerEvent === 'undefined') {
    vi.stubGlobal('PointerEvent', class extends MouseEvent {});
  }
  vi.spyOn(HTMLCanvasElement.prototype, 'getBoundingClientRect').mockReturnValue({
    left: 0,
    top: 0,
    right: 100,
    bottom: 50,
    width: 100,
    height: 50,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  });
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const label = (w: number, h: number) => `Image preview, ${w} by ${h} pixels`;

describe('the image studio editor (UTL-IMG-001)', () => {
  it('resizes keeping the proportions, and undoes it', async () => {
    render(<StudioEditor bitmap={createBitmap(100, 50, [20, 40, 60, 255])} />);
    expect(screen.getByRole('img', { name: label(100, 50) })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Resize' }));
    const width = screen.getByLabelText('Width in pixels');
    fireEvent.change(width, { target: { value: '40' } });
    expect(screen.getByLabelText('Height in pixels')).toHaveValue(20);
    const apply = screen
      .getAllByRole('button', { name: 'Resize' })
      .find((b) => !b.hasAttribute('aria-pressed'))!;
    await userEvent.click(apply);
    expect(await screen.findByRole('img', { name: label(40, 20) })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(await screen.findByRole('img', { name: label(100, 50) })).toBeInTheDocument();
  });

  it('pads to a square and rotates a quarter turn', async () => {
    render(<StudioEditor bitmap={createBitmap(100, 50)} initialMode="pad" />);
    await userEvent.click(screen.getByRole('button', { name: 'Pad to a square' }));
    expect(await screen.findByRole('img', { name: label(100, 100) })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Crop' }));
    await userEvent.click(screen.getByRole('button', { name: 'Rotate right' }));
    expect(await screen.findByRole('img', { name: label(100, 100) })).toBeInTheDocument();
  });

  it('selects an area with a drag and crops to it', async () => {
    const { container } = render(<StudioEditor bitmap={createBitmap(100, 50, [1, 2, 3, 255])} />);
    const stage = container.querySelector('.sr-st-stage')!;
    expect(screen.getByRole('button', { name: 'Crop to the selection' })).toBeDisabled();
    fireEvent.pointerDown(stage, { clientX: 10, clientY: 5, pointerId: 1 });
    fireEvent.pointerMove(stage, { clientX: 60, clientY: 35, pointerId: 1 });
    fireEvent.pointerUp(stage, { clientX: 60, clientY: 35, pointerId: 1 });
    expect(await screen.findByText('Selected area, 50 by 30 pixels')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Crop to the selection' }));
    expect(await screen.findByRole('img', { name: label(50, 30) })).toBeInTheDocument();
  });

  it('covers a selection so the exported file no longer holds its pixels', async () => {
    const secret = [201, 17, 93, 255];
    const b = createBitmap(100, 50, [128, 128, 128, 255]);
    for (let y = 10; y < 30; y++)
      for (let x = 10; x < 40; x++) b.data.set(secret, (y * 100 + x) * 4);
    const onUse = vi.fn();
    const { container } = render(<StudioEditor bitmap={b} initialMode="redact" onUse={onUse} />);
    const stage = container.querySelector('.sr-st-stage')!;
    fireEvent.pointerDown(stage, { clientX: 5, clientY: 5, pointerId: 1 });
    fireEvent.pointerMove(stage, { clientX: 50, clientY: 40, pointerId: 1 });
    fireEvent.pointerUp(stage, { clientX: 50, clientY: 40, pointerId: 1 });
    await userEvent.click(await screen.findByRole('button', { name: 'Cover with black' }));
    await userEvent.click(screen.getByRole('button', { name: 'Export' }));
    await userEvent.click(screen.getByRole('button', { name: 'Use the edited image' }));
    await waitFor(() => expect(onUse).toHaveBeenCalled());
    const file = onUse.mock.calls[0]![0] as File;
    expect(file.type).toBe('image/png');
    expect(file.name).toBe('image.png');
    expect(containsBytes(await bytesOf(file), secret)).toBe(false);
  });

  it('adds a caption and keeps the image size', async () => {
    render(<StudioEditor bitmap={createBitmap(200, 100, [9, 9, 9, 255])} initialMode="caption" />);
    await userEvent.type(screen.getByLabelText('Top text'), 'hello');
    await userEvent.click(screen.getByRole('button', { name: 'Add the caption' }));
    expect(screen.getByLabelText('Top text')).toHaveValue('');
    expect(screen.getByRole('img', { name: label(200, 100) })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Undo' })).toBeEnabled();
  });

  it('can be narrowed to some modes only', () => {
    render(<StudioEditor bitmap={createBitmap(10, 10)} modes={['palette']} />);
    expect(screen.getAllByRole('button', { pressed: true })).toHaveLength(1);
    expect(screen.queryByRole('button', { name: 'Crop' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Find the colours' })).toBeInTheDocument();
  });
});

describe('the Edit image button in the attachment tray (UTL-IMG-001)', () => {
  const pending = (name: string, type: string): PendingFile => ({
    id: name,
    file: new File(['x'], name, { type }),
    name,
    spoiler: false,
    description: '',
    progress: 0,
    status: 'queued',
  });

  it('is offered for still images only, and not while sending', () => {
    const { rerender } = render(
      <UploadTray
        files={[
          pending('a.png', 'image/png'),
          pending('b.gif', 'image/gif'),
          pending('c.mp4', 'video/mp4'),
        ]}
        busy={false}
        onChange={() => undefined}
        onRemove={() => undefined}
      />,
    );
    expect(screen.getAllByTestId('tray-edit-image')).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Edit a.png in the image studio' })).toBeEnabled();
    rerender(
      <UploadTray
        files={[pending('a.png', 'image/png')]}
        busy
        onChange={() => undefined}
        onRemove={() => undefined}
      />,
    );
    expect(screen.getByTestId('tray-edit-image')).toBeDisabled();
  });
});
