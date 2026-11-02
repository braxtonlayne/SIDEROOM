import { render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CrashBoundary } from '../CrashBoundary';
import type { CrashReport } from '../allowList';
import { parseStack } from '../frames';
import {
  MAX_REPORTS_PER_PAGE,
  classify,
  reportCrash,
  reportFor,
  resetCrashState,
  setCrashSender,
} from '../report';
import { TICK_EVENT, installCrashTelemetry } from '../install';
import { TickRing, bridgeInput, ticks } from '../ringBuffer';
import { routeNameOf } from '../routeName';

const sent: CrashReport[] = [];

beforeEach(() => {
  sent.length = 0;
  resetCrashState();
  ticks.clear();
  setCrashSender(async (r) => {
    sent.push(r);
  });
});
afterEach(() => {
  resetCrashState();
});

function errorWithStack(stack: string, name = 'Error', message = 'boom'): Error {
  const e = new Error(message);
  e.name = name;
  e.stack = stack;
  return e;
}

describe('the tick ring buffer', () => {
  it('keeps the last 60 ticks, oldest first, as enums', () => {
    let now = 0;
    const ring = new TickRing(60, () => now++);
    for (let i = 0; i < 100; i++) ring.record('game', i % 2 ? 'left' : 'free text');
    const snap = ring.snapshot();
    expect(snap).toHaveLength(60);
    expect(snap[0]!.t).toBe(40);
    expect(snap[59]!.t).toBe(99);
    expect(snap.map((s) => s.input).every((i) => i === 'left' || i === 'other')).toBe(true);
  });

  it('ignores an unknown source and names bridge commands', () => {
    const ring = new TickRing(3, () => 1);
    ring.record('evil' as never, 'left');
    expect(ring.snapshot()).toEqual([]);
    expect(bridgeInput('getChannel')).toBe('bridge_get_channel');
    expect(bridgeInput('getInstanceConnectedParticipants')).toBe('bridge_get_participants');
    expect(bridgeInput('openExternalLink')).toBe('bridge_open_link');
    expect(bridgeInput('dataSend')).toBe('bridge_data_send');
  });
});

describe('stack frames', () => {
  it('reads V8 and Gecko frames, wasm frames included', () => {
    const frames = parseStack(
      [
        'RuntimeError: unreachable',
        '    at sr_e2ee_wasm::mls::decrypt (wasm://wasm/0a1b2c3d:wasm-function[412]:0x3f4a)',
        '    at renderChannel (https://app.test/assets/index-Bx3k9Qa1.js:120:15)',
        '    at https://app.test/assets/other.js:3:4',
        'gecko@https://app.test/assets/g.js:7:8',
      ].join('\n'),
    );
    expect(frames.map((f) => f.fn)).toEqual([
      'sr_e2ee_wasm::mls::decrypt',
      'renderChannel',
      undefined,
      'gecko',
    ]);
    expect(frames[1]).toMatchObject({ line: 120, col: 15 });
  });
});

describe('classifying a failure', () => {
  it('knows a WASM trap, by type, name or frame', () => {
    expect(classify(new WebAssembly.RuntimeError('unreachable'), 'error')).toBe('wasm_panic');
    expect(classify(errorWithStack('x', 'RuntimeError'), 'error')).toBe('wasm_panic');
    expect(
      classify(errorWithStack('Error\n    at f (wasm://wasm/aa:wasm-function[1]:0x2)'), 'error'),
    ).toBe('wasm_panic');
    expect(classify(errorWithStack('x', 'GPUPipelineError'), 'error')).toBe('webgpu_panic');
    expect(classify(new Error('plain'), 'unhandled_rejection')).toBe('unhandled_rejection');
  });
});

describe('reporting', () => {
  it('copies the stack frames and the ticks, never the message', () => {
    ticks.record('activity', 'bridge_data_send');
    const err = errorWithStack(
      'RuntimeError: unreachable\n    at f (https://app.test/assets/a.js?token=SECRETVALUE:4:5)',
      'RuntimeError',
      'message body hunter2 with a token Bearer abcdefghijklmnop',
    );
    const report = reportFor(err, 'error')!;
    const text = JSON.stringify(report);
    expect(report.kind).toBe('wasm_panic');
    expect(report.frames[0]).toMatchObject({ file: 'assets/a.js', line: 4, col: 5, fn: 'f' });
    expect(report.ticks.map((t) => t.input)).toEqual(['bridge_data_send']);
    for (const s of ['hunter2', 'SECRETVALUE', 'Bearer', 'app.test', 'message body']) {
      expect(text).not.toContain(s);
    }
  });

  it('sends one report per distinct crash and at most five per page', async () => {
    const same = () => errorWithStack('Error\n    at same (https://a.test/x.js:1:1)');
    expect(await reportCrash(same(), 'error')).toBe(true);
    expect(await reportCrash(same(), 'error')).toBe(false);
    for (let i = 0; i < 10; i++) {
      await reportCrash(
        errorWithStack(`Error\n    at f${i} (https://a.test/x${i}.js:1:1)`),
        'error',
      );
    }
    expect(sent).toHaveLength(MAX_REPORTS_PER_PAGE);
  });

  it('never throws when the transport fails', async () => {
    setCrashSender(async () => {
      throw new Error('offline');
    });
    await expect(
      reportCrash(errorWithStack('Error\n    at f (https://a.test/x.js:1:1)'), 'error'),
    ).resolves.toBe(false);
  });
});

describe('the page hooks', () => {
  it('turns an unhandled error, an unhandled rejection and a tick event into report data', async () => {
    const off = installCrashTelemetry({ routeName: () => 'lounge.:guildId' });
    window.dispatchEvent(new CustomEvent(TICK_EVENT, { detail: { src: 'game', input: 'jump' } }));
    window.dispatchEvent(
      new CustomEvent(TICK_EVENT, { detail: { src: 'game', input: 'type your password' } }),
    );
    const wasm = new WebAssembly.RuntimeError('unreachable');
    wasm.stack = 'RuntimeError: unreachable\n    at f (wasm://wasm/aa:wasm-function[1]:0x2)';
    window.dispatchEvent(new ErrorEvent('error', { error: wasm, message: 'secret message' }));
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({ kind: 'wasm_panic', route: 'lounge.:guildId' });
    expect(sent[0]!.ticks.map((t) => t.input)).toEqual(['jump', 'other']);

    const rejection = new Event('unhandledrejection') as PromiseRejectionEvent;
    Object.defineProperty(rejection, 'reason', {
      value: errorWithStack('Error\n    at later (https://a.test/y.js:9:9)'),
    });
    window.dispatchEvent(rejection);
    await vi.waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1]!.kind).toBe('unhandled_rejection');
    off();
  });
});

describe('route names', () => {
  it('names the matched route, never the URL', () => {
    expect(routeNameOf('/lounge/$guildId/$channelId')).toBe('lounge.:guildId.:channelId');
    expect(routeNameOf('/admin/invite-graph')).toBe('admin.invite-graph');
    expect(routeNameOf('/')).toBe('home');
    expect(routeNameOf('__root__')).toBe('unmatched');
    expect(routeNameOf(undefined)).toBe('unmatched');
  });
});

describe('the error boundary', () => {
  it('reports a render failure and shows a plain page', async () => {
    function Bomb(): never {
      throw errorWithStack(
        'Error\n    at Bomb (https://a.test/assets/b.js:2:2)',
        'Error',
        'rendering secret',
      );
    }
    const spy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    render(
      <CrashBoundary>
        <Bomb />
      </CrashBoundary>,
    );
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /reload/i })).toBeInTheDocument();
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({ kind: 'react_boundary' });
    expect(JSON.stringify(sent[0])).not.toContain('rendering secret');
    spy.mockRestore();
  });
});
