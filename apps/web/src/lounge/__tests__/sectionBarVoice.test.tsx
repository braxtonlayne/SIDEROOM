// XC-VIS-004 / D-536: the section bar's voice status is one line; on phones it shows the channel name only, and the
// full sentence stays its accessible name.
import { render, screen } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, vi } from 'vitest';
import { resetSession, useVoiceSession } from '../../voice/session';
import { SectionBar } from '../Chrome';

vi.mock('../../study/RoomsNav', () => ({ RoomsNav: () => <nav aria-label="Rooms" /> }));

afterEach(() => resetSession());

it('shows nothing about voice until a session is connected', () => {
  render(<SectionBar />);
  expect(screen.queryByTestId('sectionbar-voice')).toBeNull();
});

it('names the channel in the status, with the full sentence as its accessible name', () => {
  useVoiceSession.setState({ status: 'connected', channelName: 'General' });
  render(<SectionBar />);
  const status = screen.getByRole('status', { name: 'Voice connected: General' });
  expect(status.querySelector('.lng-sectionbar-voice-long')?.textContent).toBe(
    'Voice connected: General',
  );
  const short = status.querySelector('.lng-sectionbar-voice-short');
  expect(short?.textContent).toBe('General');
  expect(short?.getAttribute('aria-hidden')).toBe('true');
});

it('never lets the status wrap the bar, and swaps to the short form under the phone breakpoint', () => {
  const css = readFileSync(resolve(__dirname, '../lounge.css'), 'utf8');
  const rule = (sel: string) => {
    const at = css.indexOf(`${sel} {`);
    return at < 0 ? '' : css.slice(at, css.indexOf('}', at));
  };
  expect(rule('.lng-sectionbar')).toMatch(/flex-wrap: nowrap/);
  expect(rule('.lng-sectionbar-voice-long,\n.lng-sectionbar-voice-short')).toMatch(
    /text-overflow: ellipsis/,
  );
  const phone = css.slice(
    css.indexOf('@media (max-width: 768px)', css.indexOf('.lng-sectionbar-voice-short {')),
  );
  expect(phone).toMatch(/\.lng-sectionbar-voice-short \{\s*display: block;/);
  expect(phone).toMatch(/\.lng-sectionbar-nav \{\s*gap: 10px;/);
});
