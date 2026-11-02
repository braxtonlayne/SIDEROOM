import { useT } from '../i18n';
import { FIXTURE_GAMES, type FixtureGame } from './fixtures';
import { previewHint } from './state';
import { PreviewToggle } from './PreviewToggle';
import '../arcade/arcade.css';
import './showroom.css';

function SampleCard({ game }: { game: FixtureGame }) {
  const t = useT();
  const live = game.status === 'live';
  return (
    <article className="arc-card" data-status={game.status}>
      <header>
        <h3 className="arc-card-title">{game.title}</h3>
        <span className="arc-chip">
          {live ? t('showroom.arcade.playableNow') : t('showroom.arcade.inDevelopment')}
        </span>
      </header>
      <p className="arc-blurb">{game.blurb}</p>
      <p className="arc-desc">{t('showroom.arcade.players', { players: game.players })}</p>
      <ul className="arc-flags" aria-label={game.title}>
        {game.tags.map((tag) => (
          <li key={tag} className="arc-chip">
            {tag}
          </li>
        ))}
      </ul>
      <button
        type="button"
        onClick={() => previewHint(t(live ? 'showroom.hint.play' : 'showroom.hint.notify'))}
      >
        {live ? t('showroom.arcade.play') : t('showroom.arcade.notify')}
      </button>
    </article>
  );
}

/**
 * The Arcade with sample cards (UX-SHOW-001): live builds ("Playable now") apart from showcase cards ("Preview / in
 * development"). Fixtures only: no `front` request is made and no button reaches the server.
 */
export default function PreviewArcade() {
  const t = useT();
  const live = FIXTURE_GAMES.filter((g) => g.status === 'live');
  const soon = FIXTURE_GAMES.filter((g) => g.status === 'preview');
  return (
    <main className="arc-page" data-testid="showroom-arcade">
      <h1>{t('arcade.page.title')}</h1>
      <p className="arc-dim">{t('arcade.page.intro')}</p>
      <PreviewToggle />
      <section className="arc-section" aria-labelledby="srm-live">
        <h2 id="srm-live">{t('showroom.arcade.playableNow')}</h2>
        <div className="arc-grid">
          {live.map((g) => (
            <SampleCard key={g.id} game={g} />
          ))}
        </div>
      </section>
      <section className="arc-section" aria-labelledby="srm-soon">
        <h2 id="srm-soon">{t('showroom.arcade.inDevelopment')}</h2>
        <div className="arc-grid">
          {soon.map((g) => (
            <SampleCard key={g.id} game={g} />
          ))}
        </div>
      </section>
    </main>
  );
}
