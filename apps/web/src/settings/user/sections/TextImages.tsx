import { SegmentedControl, Switch } from '@sideroom/ui';
import { useT } from '../../../i18n';
import { Card, SectionTitle } from '../parts';
import type { MediaSize, SpoilerMode } from '../prefs';
import type { PrefsFormProps } from './Appearance';
import { LoadEmbedsCard } from '../../../lounge/msg/EmbedsSettings';
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
      <LoadEmbedsCard />
      <Card id="us-attachments" title={t('settings.textImages.attachments')}>
        <Switch
          label={t('settings.textImages.inline')}
          description={t('settings.textImages.inlineDesc')}
          checked={prefs.inlineAttachments}
          onChange={(e) => setPrefs({ inlineAttachments: e.target.checked })}
        />
      </Card>
      <Card
        id="us-media-size"
        title={t('settings.textImages.mediaSize')}
        description={t('settings.textImages.mediaSizeDesc')}
      >
        <SegmentedControl
          label={t('settings.textImages.mediaSize')}
          value={prefs.inlineMediaSize}
          onValueChange={(v) => setPrefs({ inlineMediaSize: v as MediaSize })}
          options={[
            { value: 'small', label: t('settings.textImages.sizeSmall') },
            { value: 'medium', label: t('settings.textImages.sizeMedium') },
            { value: 'large', label: t('settings.textImages.sizeLarge') },
          ]}
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
          <Switch
            label={t('settings.textImages.stickerSuggestions')}
            description={t('settings.textImages.stickerSuggestionsDesc')}
            checked={prefs.stickerSuggestions}
            onChange={(e) => setPrefs({ stickerSuggestions: e.target.checked })}
          />
        </div>
      </Card>
      <Card id="us-markdown-preview" title={t('settings.textImages.markdownPreview')}>
        <Switch
          label={t('settings.textImages.markdownPreviewLabel')}
          description={t('settings.textImages.markdownPreviewDesc')}
          checked={prefs.markdownPreview}
          onChange={(e) => setPrefs({ markdownPreview: e.target.checked })}
        />
      </Card>
      <Card id="us-spoilers" title={t('settings.textImages.spoilers')}>
        <SegmentedControl
          label={t('settings.textImages.spoilerContent')}
          value={prefs.spoilers}
          onValueChange={(v) => setPrefs({ spoilers: v as SpoilerMode })}
          options={[
            { value: 'click', label: t('settings.textImages.spoilerClick') },
            { value: 'mine', label: t('settings.textImages.spoilerMine') },
            { value: 'always', label: t('settings.textImages.spoilerAlways') },
          ]}
        />
      </Card>
    </>
  );
}
