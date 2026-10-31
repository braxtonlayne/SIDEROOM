import { Button, Modal } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n';
import {
  BUTTONS,
  captureSkip,
  captureStep,
  defaultMapping,
  firstBinding,
  startCapture,
  type CaptureState,
  type CaptureTarget,
  type PadBinding,
  type PadMapping,
} from './gamepad';

const bindingText = (b: PadBinding | undefined) =>
  !b ? '-' : b.kind === 'button' ? `#${b.index}` : `${b.index}${b.dir < 0 ? '-' : '+'}`;

/**
 * The mapping screen (ACT-EMU-001): "press a key" or "press a button" for each control in turn, with skip, reset to
 * the defaults, and a table of what is mapped. The keyboard fallback is always there; a pad is optional.
 */
export function MappingDialog({
  open,
  mapping,
  onClose,
  onSave,
}: {
  open: boolean;
  mapping: PadMapping;
  onClose: () => void;
  onSave: (m: PadMapping) => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState<PadMapping>(mapping);
  const [cap, setCap] = useState<CaptureState | null>(null);
  const capRef = useRef(cap);
  capRef.current = cap;

  useEffect(() => {
    if (open) {
      setDraft(mapping);
      setCap(null);
    }
  }, [open, mapping]);

  // Keyboard capture.
  useEffect(() => {
    if (!cap || cap.target !== 'keyboard' || !cap.current) return;
    const on = (ev: KeyboardEvent) => {
      ev.preventDefault();
      ev.stopPropagation();
      if (ev.code === 'Escape') return setCap(null);
      const next = captureStep(capRef.current!, { key: ev.code });
      setCap(next);
      setDraft(next.mapping);
    };
    window.addEventListener('keydown', on, true);
    return () => window.removeEventListener('keydown', on, true);
  }, [cap?.target, cap?.current]); // eslint-disable-line react-hooks/exhaustive-deps

  // Gamepad capture: wait for every control to be released, then take the next press.
  useEffect(() => {
    if (!cap || cap.target !== 'gamepad' || !cap.current) return;
    let raf = 0;
    let armed = false;
    const loop = () => {
      const pads = navigator.getGamepads ? Array.from(navigator.getGamepads()) : [];
      let hit: PadBinding | null = null;
      for (const p of pads) if (p?.connected && !hit) hit = firstBinding(p);
      if (!hit) armed = true;
      else if (armed && capRef.current) {
        armed = false;
        const next = captureStep(capRef.current, { binding: hit });
        setCap(next);
        setDraft(next.mapping);
        return;
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [cap?.target, cap?.current]); // eslint-disable-line react-hooks/exhaustive-deps

  const begin = (target: CaptureTarget) => setCap(startCapture(target, draft));

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('games.mapping.title')}
      description={t('games.mapping.description')}
      size="lg"
      footer={
        <>
          <Button variant="ghost" onClick={() => setDraft(defaultMapping())}>
            {t('games.mapping.reset')}
          </Button>
          <Button variant="secondary" onClick={onClose}>
            {t('games.mapping.cancel')}
          </Button>
          <Button
            onClick={() => {
              onSave(draft);
              onClose();
            }}
          >
            {t('games.mapping.save')}
          </Button>
        </>
      }
    >
      <div className="emu-map-actions">
        <Button variant="secondary" onClick={() => begin('keyboard')}>
          {t('games.mapping.mapKeyboard')}
        </Button>
        <Button variant="secondary" onClick={() => begin('gamepad')}>
          {t('games.mapping.mapGamepad')}
        </Button>
      </div>
      {cap?.current && (
        <p className="emu-map-prompt" role="status">
          {cap.target === 'keyboard'
            ? t('games.mapping.pressKey', { button: t(`games.button.${cap.current}`) })
            : t('games.mapping.pressButton', { button: t(`games.button.${cap.current}`) })}{' '}
          <Button variant="link" onClick={() => setCap(captureSkip(cap))}>
            {t('games.mapping.skip')}
          </Button>
        </p>
      )}
      <table className="emu-map-table">
        <thead>
          <tr>
            <th scope="col">{t('games.mapping.colButton')}</th>
            <th scope="col">{t('games.mapping.colKeyboard')}</th>
            <th scope="col">{t('games.mapping.colGamepad')}</th>
          </tr>
        </thead>
        <tbody>
          {BUTTONS.map((b) => (
            <tr key={b} data-current={cap?.current === b || undefined}>
              <th scope="row">{t(`games.button.${b}`)}</th>
              <td>{draft.keyboard[b] ?? '-'}</td>
              <td>{bindingText(draft.gamepad[b])}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Modal>
  );
}
