// Server Settings > Server Tag (LNG-SSET-011): tag text, badge icon and colour, a preview next to a member
// name, and the "Members can choose to display" switch. Members adopt the tag in User Settings > Profiles.
import { Button, ColorPicker, Switch, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { TAG_BADGES, badgeGlyph, colorOfHex, hexOfColor, isValidTag } from '../../../lib/serverTag';
import * as api from '../api';
import { useResource } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, SectionHeader, useAction } from '../ui';

export function TagPreview({ tag, badge, color }: { tag: string; badge: number; color: number }) {
  return (
    <span className="ss-tag-chip" style={{ color: hexOfColor(color) }}>
      <span aria-hidden="true">{badgeGlyph(badge)}</span> {tag.toUpperCase()}
    </span>
  );
}

export function TagSection({ guildId }: SectionProps) {
  const t = useT();
  const res = useResource(`tag:${guildId}`, () => api.getGuildTag(guildId));
  return (
    <>
      <SectionHeader
        title={t('settings.serverTag.title')}
        description={t('settings.serverTag.description')}
      />
      <Async res={res} label={t('settings.serverTag.loading')}>
        {(tag) => <TagForm guildId={guildId} tag={tag} onSaved={(next) => res.set(next)} />}
      </Async>
    </>
  );
}

function TagForm({
  guildId,
  tag,
  onSaved,
}: {
  guildId: string;
  tag: api.GuildTag | null;
  onSaved: (t: api.GuildTag | null) => void;
}) {
  const t = useT();
  const action = useAction();
  const [text, setText] = useState(tag?.tag ?? '');
  const [badge, setBadge] = useState(tag?.badge ?? 0);
  const [color, setColor] = useState(tag?.color ?? 0x5865f2);
  const [display, setDisplay] = useState(tag?.members_can_display ?? true);

  const trimmed = text.trim();
  const valid = isValidTag(trimmed);
  const dirty =
    !tag ||
    tag.tag !== trimmed ||
    tag.badge !== badge ||
    tag.color !== color ||
    tag.members_can_display !== display;

  const save = async () => {
    const next: api.GuildTag = { tag: trimmed, badge, color, members_can_display: display };
    const out = await action.run(
      () => api.putGuildTag(guildId, next),
      t('settings.serverTag.saved'),
    );
    if (out) onSaved(out);
  };
  const remove = async () => {
    const ok = await action.run(
      () => api.deleteGuildTag(guildId).then(() => true),
      t('settings.serverTag.removed'),
    );
    if (ok) onSaved(null);
  };

  return (
    <div className="ss-card">
      <TextInput
        label={t('settings.serverTag.textLabel')}
        hint={t('settings.serverTag.textHelp')}
        value={text}
        maxLength={4}
        {...(text && !valid ? { error: t('settings.serverTag.textInvalid') } : {})}
        onChange={(e) => setText(e.target.value)}
      />
      <fieldset className="ss-badge-grid" style={{ border: 0, padding: 0 }}>
        <legend className="ss-field-label">{t('settings.serverTag.badgeLabel')}</legend>
        {TAG_BADGES.map((g, i) => (
          <button
            key={i}
            type="button"
            className="ss-badge-pick"
            aria-pressed={badge === i}
            aria-label={t('settings.serverTag.badgeOption', { n: i + 1 })}
            onClick={() => setBadge(i)}
          >
            {g}
          </button>
        ))}
      </fieldset>
      <div>
        <span className="ss-field-label">{t('settings.serverTag.colorLabel')}</span>
        <ColorPicker
          label={t('settings.serverTag.colorLabel')}
          value={hexOfColor(color)}
          onValueChange={(hex) => setColor(colorOfHex(hex))}
        />
      </div>
      <Switch
        label={t('settings.serverTag.displayLabel')}
        description={t('settings.serverTag.displayHelp')}
        checked={display}
        onChange={(e) => setDisplay(e.target.checked)}
      />
      <div>
        <span className="ss-field-label">{t('settings.serverTag.previewLabel')}</span>
        <p>
          <strong>{t('settings.serverTag.previewName')}</strong>{' '}
          {valid ? <TagPreview tag={trimmed} badge={badge} color={color} /> : null}
        </p>
      </div>
      <ActionStatus error={action.error} done={action.done} />
      <div className="ss-actions">
        <Button disabled={!valid || !dirty} loading={action.busy} onClick={() => void save()}>
          {t('settings.serverTag.save')}
        </Button>
        {tag && (
          <Button variant="danger" disabled={action.busy} onClick={() => void remove()}>
            {t('settings.serverTag.remove')}
          </Button>
        )}
      </div>
    </div>
  );
}
