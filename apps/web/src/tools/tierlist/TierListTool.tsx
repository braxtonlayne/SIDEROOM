import { useState } from 'react';
import { useT } from '../../i18n';
import { useStore } from '../../store';
import { MountImagePicker } from './MountImagePicker';
import { RoomImage } from './RoomImage';
import { RoomShell } from './RoomShell';
import {
  UNRANKED,
  addItem,
  addTier,
  defaultTiers,
  itemsIn,
  moveItem,
  parseTierList,
  removeTier,
  tierListText,
  MAX_TEXT,
  MAX_TIER_NAME,
  type ImgRef,
  type TierItem,
} from './model';
import type { LiveRoom } from './useRoom';

const DRAG_TYPE = 'application/x-sideroom-tier-item';

function ItemChip({
  item,
  tiers,
  onMove,
  onRemove,
  onDropBefore,
}: {
  item: TierItem;
  tiers: { id: string; name: string }[];
  onMove: (tier: string) => void;
  onRemove: () => void;
  onDropBefore: (dragged: string) => void;
}) {
  const t = useT();
  const label = [item.emoji, item.text].filter(Boolean).join(' ');
  return (
    <li
      className="sr-room-chip"
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData(DRAG_TYPE, item.id);
        e.dataTransfer.effectAllowed = 'move';
      }}
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes(DRAG_TYPE)) e.preventDefault();
      }}
      onDrop={(e) => {
        const id = e.dataTransfer.getData(DRAG_TYPE);
        if (id && id !== item.id) {
          e.preventDefault();
          e.stopPropagation();
          onDropBefore(id);
        }
      }}
    >
      {item.img && <RoomImage img={item.img} label={label || t('tools.tierlist.image')} />}
      {label && <span className="sr-room-chip-text">{label}</span>}
      <select
        className="sr-room-move"
        aria-label={t('tools.tierlist.moveItem', { name: label || t('tools.tierlist.image') })}
        value={item.tier}
        onChange={(e) => onMove(e.target.value)}
      >
        <option value={UNRANKED}>{t('tools.tierlist.unranked')}</option>
        {tiers.map((tier) => (
          <option key={tier.id} value={tier.id}>
            {tier.name}
          </option>
        ))}
      </select>
      <button
        type="button"
        className="sr-room-x"
        aria-label={t('tools.tierlist.removeItem', { name: label || t('tools.tierlist.image') })}
        onClick={onRemove}
      >
        ×
      </button>
    </li>
  );
}

function TierListEditor({ live, channelId }: { live: LiveRoom; channelId: string }) {
  const t = useT();
  const { view, edit } = live;
  const { tiers, items } = parseTierList(view.state);
  const [text, setText] = useState('');
  const [emoji, setEmoji] = useState('');
  const [img, setImg] = useState<{ ref: ImgRef; label: string } | null>(null);
  const [picking, setPicking] = useState(false);
  const [tierName, setTierName] = useState('');
  const [posted, setPosted] = useState(false);

  const move = (id: string, tier: string, index: number) => {
    const set = moveItem(view.state, id, tier, index);
    if (set) edit(set);
  };
  const dropOnRow = (e: React.DragEvent, tier: string) => {
    const id = e.dataTransfer.getData(DRAG_TYPE);
    if (!id) return;
    e.preventDefault();
    move(id, tier, itemsIn(items, tier).filter((i) => i.id !== id).length);
  };
  const tierOpts = tiers.map((x) => ({ id: x.id, name: x.name }));

  const zone = (tier: string, list: TierItem[], label: string) => (
    <ul
      className="sr-room-zone"
      aria-label={label}
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes(DRAG_TYPE)) e.preventDefault();
      }}
      onDrop={(e) => dropOnRow(e, tier)}
    >
      {list.map((it, idx) => (
        <ItemChip
          key={it.id}
          item={it}
          tiers={tierOpts}
          onMove={(to) => move(it.id, to, itemsIn(items, to).length)}
          onRemove={() => edit({ [`item.${it.id}`]: null })}
          onDropBefore={(dragged) => move(dragged, tier, idx)}
        />
      ))}
    </ul>
  );

  const submitItem = (e: React.FormEvent) => {
    e.preventDefault();
    const set = addItem(view.state, { text, emoji, img: img?.ref ?? null });
    if (!set) return;
    edit(set);
    setText('');
    setEmoji('');
    setImg(null);
  };

  const post = () => {
    const body = tierListText(view.title, view.state, t('tools.tierlist.unranked'));
    if (!body) return;
    useStore.getState().sendMessage(channelId, body.slice(0, 2000), null);
    setPosted(true);
  };

  return (
    <div className="sr-tier">
      {tiers.length === 0 && (
        <button
          type="button"
          className="sr-room-btn"
          onClick={() => edit(defaultTiers([t('tools.tierlist.s'), 'A', 'B', 'C', 'D']))}
        >
          {t('tools.tierlist.addDefaults')}
        </button>
      )}
      {tiers.map((tier) => (
        <section key={tier.id} className="sr-tier-row" data-hue={tier.hue}>
          <div className="sr-tier-head">
            <input
              className="sr-tier-name"
              aria-label={t('tools.tierlist.tierName')}
              value={tier.name}
              maxLength={MAX_TIER_NAME}
              onChange={(e) =>
                edit({
                  [`tier.${tier.id}`]: { name: e.target.value, hue: tier.hue, pos: tier.pos },
                })
              }
            />
            <button
              type="button"
              className="sr-room-x"
              aria-label={t('tools.tierlist.removeTier', { name: tier.name })}
              onClick={() => edit(removeTier(view.state, tier.id))}
            >
              ×
            </button>
          </div>
          {zone(tier.id, itemsIn(items, tier.id), tier.name || t('tools.tierlist.tierName'))}
        </section>
      ))}

      <form
        className="sr-room-inline"
        onSubmit={(e) => {
          e.preventDefault();
          const set = addTier(view.state, tierName);
          if (set) {
            edit(set);
            setTierName('');
          }
        }}
      >
        <input
          aria-label={t('tools.tierlist.newTier')}
          placeholder={t('tools.tierlist.newTier')}
          value={tierName}
          maxLength={MAX_TIER_NAME}
          onChange={(e) => setTierName(e.target.value)}
        />
        <button type="submit" className="sr-room-btn">
          {t('tools.tierlist.addTier')}
        </button>
      </form>

      <h3 className="sr-room-h">{t('tools.tierlist.unranked')}</h3>
      {zone(UNRANKED, itemsIn(items, UNRANKED), t('tools.tierlist.unranked'))}

      <form className="sr-room-inline" onSubmit={submitItem}>
        <input
          aria-label={t('tools.tierlist.itemText')}
          placeholder={t('tools.tierlist.itemText')}
          value={text}
          maxLength={MAX_TEXT}
          onChange={(e) => setText(e.target.value)}
        />
        <input
          className="sr-room-emoji"
          aria-label={t('tools.tierlist.itemEmoji')}
          placeholder={t('tools.tierlist.itemEmoji')}
          value={emoji}
          maxLength={8}
          onChange={(e) => setEmoji(e.target.value)}
        />
        <button type="button" className="sr-room-btn" onClick={() => setPicking(true)}>
          {img
            ? t('tools.tierlist.imagePicked', { name: img.label })
            : t('tools.tierlist.pickImage')}
        </button>
        <button type="submit" className="sr-room-btn sr-room-primary">
          {t('tools.tierlist.addItem')}
        </button>
      </form>
      {picking && (
        <MountImagePicker
          onClose={() => setPicking(false)}
          onPick={(ref, label) => {
            setImg({ ref, label });
            setPicking(false);
          }}
        />
      )}

      <div className="sr-room-inline">
        <button type="button" className="sr-room-btn" onClick={post} disabled={tiers.length === 0}>
          {t('tools.tierlist.post')}
        </button>
        {posted && (
          <span role="status" className="sr-tl-muted">
            {t('tools.tierlist.posted')}
          </span>
        )}
      </div>
    </div>
  );
}

/** UTL-COL-001: the collaborative tier list. */
export default function TierListTool() {
  const t = useT();
  return (
    <RoomShell
      kind="tierlist"
      defaultTitle={t('tools.tierlist.defaultTitle')}
      starter={() => defaultTiers([t('tools.tierlist.s'), 'A', 'B', 'C', 'D'])}
    >
      {(live, ctx) => <TierListEditor live={live} channelId={ctx.channelId} />}
    </RoomShell>
  );
}
