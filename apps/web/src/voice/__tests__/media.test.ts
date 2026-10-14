import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { micConstraints, MicError, toMicError } from '../deps';
import { MicPipeline, METER_INTERVAL_MS } from '../mic';
import { RemoteAudio } from '../playback';
import { CuePlayer } from '../sounds';
import { FakeAnalyser, FakeAudioContext, FakeGainNode, FakeStream, fakeAudio } from './fakes';

const mic = {
  deviceId: 'default',
  echoCancellation: true,
  autoGainControl: true,
  inputVolume: 100,
};

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('microphone constraints', () => {
  it('always disables noise suppression and carries the user choices (D-045)', () => {
    const c = micConstraints({
      deviceId: 'default',
      echoCancellation: true,
      autoGainControl: false,
    });
    const a = c.audio as MediaTrackConstraints;
    expect(a.noiseSuppression).toBe(false);
    expect(a.echoCancellation).toBe(true);
    expect(a.autoGainControl).toBe(false);
    expect(a.channelCount).toBe(1);
    expect(a.deviceId).toBeUndefined();
    const picked = micConstraints({
      deviceId: 'abc',
      echoCancellation: false,
      autoGainControl: true,
    });
    expect((picked.audio as MediaTrackConstraints).deviceId).toEqual({ exact: 'abc' });
    expect(picked.video).toBe(false);
  });

  it('maps getUserMedia failures to actionable errors', () => {
    expect(toMicError(new DOMException('x', 'NotAllowedError')).code).toBe('permission-denied');
    expect(toMicError(new DOMException('x', 'NotFoundError')).code).toBe('no-device');
    expect(toMicError(new DOMException('x', 'NotReadableError')).code).toBe('in-use');
    expect(toMicError(new Error('boom')).code).toBe('unknown');
    const own = new MicError('in-use', 'x');
    expect(toMicError(own)).toBe(own);
  });
});

describe('MicPipeline', () => {
  it('builds source -> analyser -> gain -> destination and sends the processed track', async () => {
    const w = fakeAudio();
    const p = new MicPipeline(w.deps);
    const track = await p.start({ ...mic, inputVolume: 150 });
    const ctx = w.contexts[0]!;
    expect(ctx.analysers).toHaveLength(1);
    expect(ctx.gains[0]!.gain.value).toBe(1.5);
    expect(track).toBe(ctx.dests[0]!.stream.getAudioTracks()[0]);
    expect(p.outStream).toBe(ctx.dests[0]!.stream);
    p.setInputVolume(0);
    expect(ctx.gains[0]!.gain.value).toBe(0);
    p.setInputVolume(200);
    expect(ctx.gains[0]!.gain.value).toBe(2);
  });

  it('requests the device with echo cancellation and AGC but never noise suppression', async () => {
    const w = fakeAudio();
    await new MicPipeline(w.deps).start({ ...mic, deviceId: 'usb-1', autoGainControl: false });
    const arg = w.gum.mock.calls[0]![0] as MediaStreamConstraints;
    const audio = arg.audio as MediaTrackConstraints;
    expect(audio.noiseSuppression).toBe(false);
    expect(audio.autoGainControl).toBe(false);
    expect(audio.echoCancellation).toBe(true);
    expect(audio.deviceId).toEqual({ exact: 'usb-1' });
  });

  it('falls back to the default device when the saved one is gone', async () => {
    const w = fakeAudio();
    w.gum.mockRejectedValueOnce(Object.assign(new Error('gone'), { name: 'OverconstrainedError' }));
    await new MicPipeline(w.deps).start({ ...mic, deviceId: 'unplugged' });
    expect(w.gum).toHaveBeenCalledTimes(2);
    expect(
      ((w.gum.mock.calls[1]![0] as MediaStreamConstraints).audio as MediaTrackConstraints).deviceId,
    ).toBeUndefined();
  });

  it('reports permission denial', async () => {
    const w = fakeAudio();
    w.gum.mockRejectedValue(new DOMException('no', 'NotAllowedError'));
    await expect(new MicPipeline(w.deps).start(mic)).rejects.toMatchObject({
      code: 'permission-denied',
    });
  });

  it('gates the outgoing track and keeps the gate across a restart', async () => {
    const w = fakeAudio();
    const p = new MicPipeline(w.deps);
    p.setGate(false);
    const track = (await p.start(mic)) as unknown as { enabled: boolean };
    expect(track.enabled).toBe(false);
    p.setGate(true);
    expect(track.enabled).toBe(true);
  });

  it('samples the level on an interval and stops cleanly', async () => {
    const w = fakeAudio();
    const p = new MicPipeline(w.deps);
    await p.start(mic);
    const seen: number[] = [];
    p.onLevel((db) => seen.push(db));
    (w.contexts[0]!.analysers[0] as FakeAnalyser).amplitude = 0.1;
    vi.advanceTimersByTime(METER_INTERVAL_MS * 3);
    expect(seen).toHaveLength(3);
    expect(seen[0]).toBeCloseTo(-20, 1);
    p.stop();
    vi.advanceTimersByTime(METER_INTERVAL_MS * 3);
    expect(seen).toHaveLength(3);
    expect(
      w.streams[0]!.getTracks().every((t) => (t as unknown as { stopped: boolean }).stopped),
    ).toBe(true);
    expect(w.contexts[0]!.closed).toBe(true);
    expect(p.active).toBe(false);
  });

  it('tells listeners when the device disappears', async () => {
    const w = fakeAudio();
    const p = new MicPipeline(w.deps);
    await p.start(mic);
    const lost = vi.fn();
    p.onLost(lost);
    (w.streams[0]!.getAudioTracks()[0] as unknown as { fireEnded(): void }).fireEnded();
    expect(lost).toHaveBeenCalledOnce();
  });

  it('works without WebAudio by sending the raw track', async () => {
    const w = fakeAudio({ webAudio: false });
    const p = new MicPipeline(w.deps);
    const track = await p.start(mic);
    expect(track).toBe(w.streams[0]!.getAudioTracks()[0]);
    p.setGate(false);
    expect((track as unknown as { enabled: boolean }).enabled).toBe(false);
  });
});

describe('RemoteAudio', () => {
  const stream = () => new FakeStream() as unknown as MediaStream;

  it('plays through a gain stage and goes to 200% per person', () => {
    const w = fakeAudio();
    const r = new RemoteAudio(w.deps);
    r.add('u1', stream());
    r.add('u2', stream());
    const ctx = w.contexts[0]!;
    const gains = ctx.gains.slice(1) as FakeGainNode[]; // gains[0] is the master
    expect(gains).toHaveLength(2);
    r.setVolume('u1', 200);
    r.setVolume('u2', 0);
    expect(gains[0]!.gain.value).toBe(2);
    expect(gains[1]!.gain.value).toBe(0);
    // The element only keeps the stream flowing.
    expect(w.elements.every((e) => e.muted)).toBe(true);
  });

  it('local mute silences one person and restores their volume', () => {
    const w = fakeAudio();
    const r = new RemoteAudio(w.deps);
    r.add('u1', stream(), 150);
    const g = w.contexts[0]!.gains[1]!;
    expect(g.gain.value).toBe(1.5);
    r.setMuted('u1', true);
    expect(g.gain.value).toBe(0);
    r.setMuted('u1', false);
    expect(g.gain.value).toBe(1.5);
  });

  it('deafen silences every remote voice and undeafen restores them', () => {
    const w = fakeAudio();
    const r = new RemoteAudio(w.deps);
    r.add('u1', stream());
    r.add('u2', stream(), 80);
    const [master, g1, g2] = w.contexts[0]!.gains;
    r.setDeafened(true);
    expect(master!.gain.value).toBe(0);
    expect(g1!.gain.value).toBe(0);
    expect(g2!.gain.value).toBe(0);
    expect(w.elements.every((e) => e.muted)).toBe(true);
    // A person joining while deafened is silent too.
    r.add('u3', stream());
    expect(w.contexts[0]!.gains[3]!.gain.value).toBe(0);
    r.setDeafened(false);
    expect(master!.gain.value).toBe(1);
    expect(g2!.gain.value).toBe(0.8);
  });

  it('deafen mutes the elements when WebAudio is unavailable', () => {
    const w = fakeAudio({ webAudio: false });
    const r = new RemoteAudio(w.deps);
    r.add('u1', stream(), 50);
    r.add('u2', stream());
    expect(w.elements.map((e) => e.muted)).toEqual([false, false]);
    expect(w.elements[0]!.volume).toBe(0.5);
    r.setDeafened(true);
    expect(w.elements.every((e) => e.muted)).toBe(true);
    r.setDeafened(false);
    expect(w.elements.every((e) => !e.muted)).toBe(true);
    r.setVolume('u1', 200);
    expect(w.elements[0]!.volume).toBe(1);
  });

  it('master volume scales everyone', () => {
    const w = fakeAudio();
    const r = new RemoteAudio(w.deps);
    r.add('u1', stream());
    r.setMasterVolume(50);
    expect(w.contexts[0]!.gains[0]!.gain.value).toBe(0.5);
  });

  it('removes a person and routes output to a chosen device', async () => {
    const w = fakeAudio();
    const r = new RemoteAudio(w.deps);
    r.add('u1', stream());
    await r.setOutputDevice('speakers-2');
    expect(w.elements[0]!.sinkId).toBe('speakers-2');
    expect(w.contexts[0]!.sinkId).toBe('speakers-2');
    r.remove('u1');
    expect(r.has('u1')).toBe(false);
    expect(w.elements[0]!.srcObject).toBeNull();
    r.dispose();
    expect(w.contexts[0]!.closed).toBe(true);
  });
});

describe('CuePlayer', () => {
  it('plays one oscillator per tone and respects volume zero / no WebAudio', () => {
    const ctx = new FakeAudioContext();
    const p = new CuePlayer(() => ctx as unknown as AudioContext);
    expect(p.play('selfJoin', 0.5)).toBe(true);
    expect(ctx.oscillators).toBe(3);
    expect(p.play('join', 0)).toBe(false);
    expect(new CuePlayer(() => null).play('join', 1)).toBe(false);
  });
});
