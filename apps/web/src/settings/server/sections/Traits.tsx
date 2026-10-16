// Server traits editor (J-07, LNG-SSET-007): up to five slots, each an emoji and a short label.
import { Button, TextInput } from '@sideroom/ui';
import { useT } from '../../../i18n';
import { TraitChips } from '../../../lounge/components/TraitChips';
import { TRAITS_MAX, TRAIT_LABEL_MAX, type GuildTrait } from '../api';

/** Characters the server refuses in a label (mentions and markdown), mirrored here so the editor can say so. */
const FORBIDDEN = /[@<>*_~`|\\#[\]]/;

/** The first problem with a label, as a catalog key, or undefined. */
export function traitError(label: string): 'empty' | 'chars' | undefined {
  const l = label.trim();
  if (!l) return 'empty';
  if (FORBIDDEN.test(l)) return 'chars';
  return undefined;
}

/** True when every trait can be saved. */
export const traitsValid = (traits: readonly GuildTrait[]) =>
  traits.length <= TRAITS_MAX && traits.every((t) => !traitError(t.label));

export function TraitsEditor({
  traits,
  onChange,
}: {
  traits: GuildTrait[];
  onChange: (next: GuildTrait[]) => void;
}) {
  const t = useT();
  const set = (i: number, patch: Partial<GuildTrait>) =>
    onChange(traits.map((tr, n) => (n === i ? { ...tr, ...patch } : tr)));
  return (
    <section className="ss-card" aria-labelledby="ss-traits">
      <h2 id="ss-traits">{t('settings.traits.title')}</h2>
      <p className="ss-sub">{t('settings.traits.hint', { max: TRAITS_MAX })}</p>
      <ul className="ss-list" aria-label={t('settings.traits.listLabel')}>
        {traits.map((tr, i) => {
          const err = traitError(tr.label);
          const customEmoji = !!tr.emoji_id;
          return (
            <li key={i} className="ss-row">
              <TextInput
                label={t('settings.traits.emojiLabel', { n: i + 1 })}
                value={customEmoji ? `:${tr.emoji_name ?? ''}:` : (tr.emoji_name ?? '')}
                maxLength={16}
                disabled={customEmoji}
                onChange={(e) => set(i, { emoji_id: null, emoji_name: e.target.value })}
              />
              <div className="ss-grow">
                <TextInput
                  label={t('settings.traits.labelLabel', { n: i + 1 })}
                  value={tr.label}
                  maxLength={TRAIT_LABEL_MAX}
                  error={
                    err === 'chars'
                      ? t('settings.traits.errorChars')
                      : err === 'empty' && tr.label.length > 0
                        ? t('settings.traits.errorEmpty')
                        : undefined
                  }
                  onChange={(e) => set(i, { label: e.target.value })}
                />
              </div>
              <Button
                size="sm"
                variant="ghost"
                aria-label={t('settings.traits.remove', { n: i + 1 })}
                onClick={() => onChange(traits.filter((_, n) => n !== i))}
              >
                {t('settings.traits.removeShort')}
              </Button>
            </li>
          );
        })}
      </ul>
      <div className="ss-actions">
        <Button
          variant="secondary"
          disabled={traits.length >= TRAITS_MAX}
          onClick={() => onChange([...traits, { label: '' }])}
        >
          {t('settings.traits.add')}
        </Button>
      </div>
      <TraitChips
        traits={traits.filter((tr) => tr.label.trim())}
        label={t('settings.traits.previewLabel')}
      />
    </section>
  );
}
