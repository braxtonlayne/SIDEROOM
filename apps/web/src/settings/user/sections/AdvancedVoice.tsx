import { Notice, Switch } from '@sideroom/ui';
import { useMemo } from 'react';
import { useT } from '../../../i18n';
import { useVoiceSettings } from '../../../voice/settings';
import { Card } from '../parts';

/** Names a browser reports for a software renderer: no GPU is doing the drawing. */
const SOFTWARE_RENDERER = /swiftshader|llvmpipe|software|basic render|softpipe/i;

/** `true` hardware, `false` software, `null` when the browser will not say (privacy modes hide it). */
export function detectHardwareAcceleration(
  makeCanvas: () => HTMLCanvasElement = () => document.createElement('canvas'),
): boolean | null {
  try {
    const gl = (makeCanvas().getContext('webgl') ?? null) as WebGLRenderingContext | null;
    if (!gl) return false;
    const ext = gl.getExtension('WEBGL_debug_renderer_info');
    if (!ext) return null;
    const name = String(gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) ?? '');
    if (!name) return null;
    return !SOFTWARE_RENDERER.test(name);
  } catch {
    return null;
  }
}

/** User Settings > Advanced: the hardware acceleration hint, QoS and the encryption info (LNG-USET-050). */
export function AdvancedVoiceCards() {
  const t = useT();
  const settings = useVoiceSettings((s) => s.settings);
  const update = useVoiceSettings((s) => s.update);
  const accel = useMemo(() => detectHardwareAcceleration(), []);
  return (
    <>
      <Card
        id="us-hw-accel"
        title={t('settings.advanced.hwTitle')}
        description={t('settings.advanced.hwDesc')}
      >
        {accel === false ? (
          <Notice variant="info" title={t('settings.advanced.hwOffTitle')}>
            {t('settings.advanced.hwOffBody')}
          </Notice>
        ) : (
          <p className="us-muted">
            {accel ? t('settings.advanced.hwOn') : t('settings.advanced.hwUnknown')}
          </p>
        )}
      </Card>
      <Card id="us-qos" title={t('settings.advanced.qosTitle')}>
        <Switch
          label={t('settings.advanced.qosLabel')}
          description={t('settings.advanced.qosDesc')}
          checked={settings.qosHighPriority}
          onChange={(e) => update({ qosHighPriority: e.target.checked })}
        />
      </Card>
      <Card id="us-e2ee-info" title={t('settings.advanced.e2eeTitle')}>
        {/* Always on (OPS-IMM-001): no setting, flag or build option turns call encryption off. */}
        <Notice variant="info" title={t('settings.advanced.e2eeOn')}>
          {t('settings.advanced.e2eeOnBody')}
        </Notice>
        <Switch
          label={t('settings.advanced.privacyCodes')}
          description={t('settings.advanced.privacyCodesDesc')}
          checked={settings.showPrivacyCodes}
          onChange={(e) => update({ showPrivacyCodes: e.target.checked })}
        />
      </Card>
    </>
  );
}
