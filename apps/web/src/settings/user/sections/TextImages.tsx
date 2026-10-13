import { SegmentedControl, Switch } from '@sideroom/ui';
import { Card, SectionTitle } from '../parts';
import type { SpoilerMode } from '../prefs';
import type { PrefsFormProps } from './Appearance';
import { RemoteImagesCard } from './Privacy';

export function TextImagesSection({ prefs, setPrefs }: PrefsFormProps) {
  return (
    <>
      <SectionTitle>Text &amp; Images</SectionTitle>
      <Card id="us-previews" title="Links and embeds">
        <div className="us-stack">
          <Switch
            label="Show website previews for links"
            description="Titles, descriptions and pictures under links, and cards for invites and message links."
            checked={prefs.linkPreviews}
            onChange={(e) => setPrefs({ linkPreviews: e.target.checked })}
          />
          <Switch
            label="Show embeds from apps and bots"
            checked={prefs.showEmbeds}
            onChange={(e) => setPrefs({ showEmbeds: e.target.checked })}
          />
        </div>
      </Card>
      <RemoteImagesCard />
      <Card id="us-attachments" title="Attachments">
        <Switch
          label="Show attachments directly in chat"
          description="Pictures and videos open inline. Turn it off to see them as file cards you open on purpose."
          checked={prefs.inlineAttachments}
          onChange={(e) => setPrefs({ inlineAttachments: e.target.checked })}
        />
      </Card>
      <Card id="us-emoji" title="Emoji and reactions">
        <div className="us-stack">
          <Switch
            label="Show emoji reactions on messages"
            checked={prefs.showReactions}
            onChange={(e) => setPrefs({ showReactions: e.target.checked })}
          />
          <Switch
            label="Convert emoticons to emoji"
            description="Typing :) sends a smiley. Text with code in it is left alone."
            checked={prefs.convertEmoticons}
            onChange={(e) => setPrefs({ convertEmoticons: e.target.checked })}
          />
        </div>
      </Card>
      <Card id="us-spoilers" title="Spoilers">
        <SegmentedControl
          label="Spoiler content"
          value={prefs.spoilers}
          onValueChange={(v) => setPrefs({ spoilers: v as SpoilerMode })}
          options={[
            { value: 'click', label: 'Hide until clicked' },
            { value: 'always', label: 'Always show' },
          ]}
        />
      </Card>
    </>
  );
}
