import { useState } from 'react';
import { useT } from '../../i18n';
import { Root } from '../dev/parts';
import { flipCoin } from './engine';

/** A coin: heads or tails, with a running tally. */
export default function CoinTool() {
  const t = useT();
  const [last, setLast] = useState<boolean | null>(null);
  const [tally, setTally] = useState({ heads: 0, tails: 0 });

  const flip = async () => {
    const heads = await flipCoin();
    setLast(heads);
    setTally((c) => (heads ? { ...c, heads: c.heads + 1 } : { ...c, tails: c.tails + 1 }));
  };

  return (
    <Root label={t('tools.coin.name')}>
      <div className="ud-coin" role="status" aria-live="polite">
        {last === null ? '?' : last ? t('tools.coin.heads') : t('tools.coin.tails')}
      </div>
      <div className="ud-row" style={{ justifyContent: 'center' }}>
        <button type="button" className="ud-btn ud-btn-primary" onClick={() => void flip()}>
          {t('tools.coin.flip')}
        </button>
        <button
          type="button"
          className="ud-btn"
          onClick={() => {
            setLast(null);
            setTally({ heads: 0, tails: 0 });
          }}
        >
          {t('tools.coin.reset')}
        </button>
      </div>
      <p className="ud-muted" style={{ textAlign: 'center' }}>
        {t('tools.coin.tally', { heads: tally.heads, tails: tally.tails })}
      </p>
    </Root>
  );
}
