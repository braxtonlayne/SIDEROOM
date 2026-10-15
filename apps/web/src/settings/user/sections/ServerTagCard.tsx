// User Settings > Profiles > Server tag (LNG-SSET-012): pick which server's tag to display next to your name.
// Only servers that have a tag and allow members to display it are offered; "None" clears the choice.
import { Notice, Select } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { badgeGlyph, hexOfColor } from '../../../lib/serverTag';
import {
  errorText,
  getPrimaryGuild,
  listGuildTags,
  putPrimaryGuild,
  useResource,
  type GuildTagChoice,
} from '../api';
import { Card, Loaded } from '../parts';

const NONE = 'none';

async function load(): Promise<{ choices: GuildTagChoice[]; current: string | null }> {
  const [choices, current] = await Promise.all([listGuildTags(), getPrimaryGuild()]);
  return { choices, current: current?.identity_guild_id ?? null };
}

export function ServerTagCard() {
  const t = useT();
  const res = useResource(load);
  const [picked, setPicked] = useState<string | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const choose = async (value: string) => {
    setError(null);
    setSaved(false);
    const guildId = value === NONE ? null : value;
    try {
      await putPrimaryGuild(guildId);
      setPicked(guildId);
      setSaved(true);
    } catch (e) {
      setError(errorText(e, t('settings.serverTagPicker.error')));
    }
  };

  return (
    <Card
      id="us-server-tag"
      title={t('settings.serverTagPicker.title')}
      description={t('settings.serverTagPicker.description')}
    >
      <Loaded res={res} what={t('settings.serverTagPicker.what')}>
        {({ choices, current }) => {
          if (choices.length === 0)
            return <p className="us-muted">{t('settings.serverTagPicker.none')}</p>;
          const value = (picked === undefined ? current : picked) ?? NONE;
          const shown = choices.find((c) => c.guildId === value);
          return (
            <div className="us-stack">
              <label className="us-muted" id="us-tag-label">
                {t('settings.serverTagPicker.label')}
              </label>
              <Select
                aria-labelledby="us-tag-label"
                options={[
                  { value: NONE, label: t('settings.serverTagPicker.noTag') },
                  ...choices.map((c) => ({
                    value: c.guildId,
                    label: `${c.guildName} (${badgeGlyph(c.badge)} ${c.tag.toUpperCase()})`,
                  })),
                ]}
                value={value}
                onValueChange={(v) => void choose(v)}
              />
              {shown && (
                <p>
                  <strong>{t('settings.serverTagPicker.previewName')}</strong>{' '}
                  <span className="ss-tag-chip" style={{ color: hexOfColor(shown.color) }}>
                    <span aria-hidden="true">{badgeGlyph(shown.badge)}</span>{' '}
                    {shown.tag.toUpperCase()}
                  </span>
                </p>
              )}
              {saved && (
                <p className="us-muted" role="status">
                  {t('settings.serverTagPicker.saved')}
                </p>
              )}
              {error && (
                <Notice variant="danger" role="alert">
                  {error}
                </Notice>
              )}
            </div>
          );
        }}
      </Loaded>
    </Card>
  );
}
