import { Button, Icon } from '@sideroom/ui';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../../i18n';
import { formatDuration } from './format';
import { decodeAudioFile, durationOf } from '../../settings/server/sections/soundTrim';
import { AudioTool } from '../../tools/audio/AudioTool';
import { FLAT_WAVEFORM, MAX_VOICE_SECS, voiceMimeType, waveformOf } from './voice';

export interface VoiceClip {
  file: File;
  durationSecs: number;
  waveform: string;
}

type Phase = 'starting' | 'recording' | 'processing' | 'preview' | 'trim' | 'error';

/**
 * Voice message recorder bar (LNG-CMP-012): record, stop, listen, send or discard. It starts as soon as it
 * mounts (the click on the mic button); on touch screens the button is held instead, and the parent calls
 * `stopRef.current()` on release.
 */
export function VoiceRecorder({
  onSend,
  onCancel,
  stopRef,
}: {
  onSend: (clip: VoiceClip) => void;
  onCancel: () => void;
  stopRef?: { current: (() => void) | null };
}) {
  const t = useT();
  const [phase, setPhase] = useState<Phase>('starting');
  /** Catalog key of the error message. */
  const [error, setError] = useState<string | null>(null);
  const [secs, setSecs] = useState(0);
  const [clip, setClip] = useState<VoiceClip | null>(null);
  const [url, setUrl] = useState<string | null>(null);
  const rec = useRef<MediaRecorder | null>(null);
  const stream = useRef<MediaStream | null>(null);
  const chunks = useRef<Blob[]>([]);
  const startedAt = useRef(0);
  const cancelled = useRef(false);

  const release = useCallback(() => {
    stream.current?.getTracks().forEach((t) => t.stop());
    stream.current = null;
  }, []);

  const stop = useCallback(() => {
    if (rec.current && rec.current.state !== 'inactive') rec.current.stop();
  }, []);
  useEffect(() => {
    if (stopRef) stopRef.current = stop;
    return () => {
      if (stopRef) stopRef.current = null;
    };
  }, [stop, stopRef]);

  useEffect(() => {
    const mime = voiceMimeType();
    if (!mime || !navigator.mediaDevices?.getUserMedia) {
      setError('lounge.voiceRecorder.unsupported');
      setPhase('error');
      return;
    }
    let live = true;
    navigator.mediaDevices
      .getUserMedia({ audio: true })
      .then((s) => {
        if (!live) {
          s.getTracks().forEach((t) => t.stop());
          return;
        }
        stream.current = s;
        const r = new MediaRecorder(s, { mimeType: mime });
        rec.current = r;
        r.ondataavailable = (e) => {
          if (e.data.size > 0) chunks.current.push(e.data);
        };
        r.onstop = () => {
          release();
          if (cancelled.current) return;
          const durationSecs = Math.min(
            MAX_VOICE_SECS,
            Math.max(0.1, (Date.now() - startedAt.current) / 1000),
          );
          const blob = new Blob(chunks.current, { type: 'audio/ogg' });
          setPhase('processing');
          void waveformOf(blob).then((waveform) => {
            if (cancelled.current) return;
            const file = new File([blob], 'voice-message.ogg', { type: 'audio/ogg' });
            setClip({ file, durationSecs, waveform: waveform || FLAT_WAVEFORM });
            setUrl(URL.createObjectURL(blob));
            setPhase('preview');
          });
        };
        startedAt.current = Date.now();
        r.start();
        setPhase('recording');
      })
      .catch(() => {
        if (!live) return;
        setError('lounge.voiceRecorder.micBlocked');
        setPhase('error');
      });
    return () => {
      live = false;
      cancelled.current = true;
      if (rec.current && rec.current.state !== 'inactive') rec.current.stop();
      release();
    };
  }, [release]);

  useEffect(() => () => void (url && URL.revokeObjectURL(url)), [url]);

  /** The trimmed part replaces the recording (UTL-AUD-001); the same Ogg and two-minute rules apply. */
  const acceptTrimmed = async (file: File) => {
    let durationSecs = clip?.durationSecs ?? 0.1;
    try {
      durationSecs = Math.min(MAX_VOICE_SECS, durationOf(await decodeAudioFile(file)));
    } catch {
      /* keep the old duration */
    }
    const waveform = (await waveformOf(file)) || FLAT_WAVEFORM;
    setClip({ file, durationSecs, waveform });
    setUrl(URL.createObjectURL(file));
    setPhase('preview');
  };

  // Elapsed time, and the two-minute cap.
  useEffect(() => {
    if (phase !== 'recording') return;
    const timer = setInterval(() => {
      const s = (Date.now() - startedAt.current) / 1000;
      setSecs(s);
      if (s >= MAX_VOICE_SECS) stop();
    }, 200);
    return () => clearInterval(timer);
  }, [phase, stop]);

  return (
    <div className="lng-voice" role="group" aria-label={t('lounge.voiceRecorder.voiceMessage')}>
      {phase === 'error' && (
        <p role="alert" className="lng-voice-text">
          {error && t(error)}
        </p>
      )}
      {(phase === 'starting' || phase === 'processing') && (
        <p role="status" className="lng-voice-text">
          {phase === 'starting'
            ? t('lounge.voiceRecorder.waitingForMic')
            : t('lounge.voiceRecorder.preparing')}
        </p>
      )}
      {phase === 'recording' && (
        <>
          <span className="lng-voice-dot" aria-hidden="true" />
          <span
            role="timer"
            aria-label={t('lounge.voiceRecorder.recordingTime')}
            className="lng-voice-text"
          >
            {formatDuration(secs)} / {formatDuration(MAX_VOICE_SECS)}
          </span>
          <Button size="sm" onClick={stop}>
            {t('lounge.voiceRecorder.stop')}
          </Button>
        </>
      )}
      {phase === 'preview' && clip && url && (
        <>
          <audio controls src={url} aria-label={t('lounge.voiceRecorder.preview')} />
          <Button size="sm" variant="secondary" onClick={() => setPhase('trim')}>
            {t('lounge.voiceRecorder.trim')}
          </Button>
          <Button size="sm" onClick={() => onSend(clip)}>
            <Icon name="arrow-right" size={16} /> {t('lounge.voiceRecorder.send')}
          </Button>
        </>
      )}
      {phase === 'trim' && clip && (
        <AudioTool
          file={clip.file}
          limits={{ maxSeconds: MAX_VOICE_SECS, format: 'ogg' }}
          onUse={(f) => void acceptTrimmed(f)}
          onCancel={() => setPhase('preview')}
        />
      )}
      <Button size="sm" variant="ghost" onClick={onCancel}>
        {phase === 'preview' ? t('lounge.voiceRecorder.delete') : t('lounge.voiceRecorder.cancel')}
      </Button>
    </div>
  );
}
