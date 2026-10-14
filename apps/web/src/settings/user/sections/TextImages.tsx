import { SegmentedControl, Switch } from '@sideroom/ui';
import { useT } from '../../../i18n';
import { Card, SectionTitle } from '../parts';
import type { SpoilerMode } from '../prefs';
import type { PrefsFormProps } from './Appearance';
import { RemoteImagesCard } from './Privacy';

export function TextImagesSection({ prefs, setPrefs }: PrefsFormProps) {
  const t = useT();
  return (
    <>
      <SectionTitle>{t('settings.textImages.title')}</SectionTitle>
      <Card id="us-previews" title={t('settings.textImages.linksEmbeds')}>
        <div className="us-stack">
          <Switch
            label={t('settings.textImages.previews')}
            description={t('settings.textImages.previewsDesc')}
            checked={prefs.linkPreviews}
            onChange={(e) => setPrefs({ linkPreviews: e.target.checked })}
          />
          <Switch
            label={t('settings.textImages.embeds')}
            checked={prefs.showEmbeds}
            onChange={(e) => setPrefs({ showEmbeds: e.target.checked })}
          />
        </div>
      </Card>
      <RemoteImagesCard />
      <Card id="us-attachments" title={t('settings.textImages.attachments')}>
        <Switch
          label={t('settings.textImages.inline')}
          description={t('settings.textImages.inlineDesc')}
          checked={prefs.inlineAttachments}
          onChange={(e) => setPrefs({ inlineAttachments: e.target.checked })}
        />
      </Card>
      <Card id="us-emoji" title={t('settings.textImages.emojiReactions')}>
        <div className="us-stack">
          <Switch
            label={t('settings.textImages.reactions')}
            checked={prefs.showReactions}
            onChange={(e) => setPrefs({ showReactions: e.target.checked })}
          />
          <Switch
            label={t('settings.textImages.emoticons')}
            description={t('settings.textImages.emoticonsDesc')}
            checked={prefs.convertEmoticons}
            onChange={(e) => setPrefs({ convertEmoticons: e.target.checked })}
          />
        </div>
      </Card>
      <Card id="us-spoilers" title={t('settings.textImages.spoilers')}>
        <SegmentedControl
          label={t('settings.textImages.spoilerContent')}
          value={prefs.spoilers}
          onValueChange={(v) => setPrefs({ spoilers: v as SpoilerMode })}
          options={[
            { value: 'click', label: t('settings.textImages.spoilerClick') },
            { value: 'always', label: t('settings.textImages.spoilerAlways') },
          ]}
        />
      </Card>
    </>
  );
}
