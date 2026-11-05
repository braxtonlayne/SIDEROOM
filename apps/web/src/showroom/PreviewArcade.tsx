import { Button, Icon } from '@sideroom/ui';
import { ArcadeHero } from '../arcade/ArcadeHero';
import { BuiltinCard, SectionHead } from '../arcade/ArcadeSections';
import { builtinsByStatus } from '../arcade/builtins';
import { GameCard } from '../arcade/GameCard';
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
    <GameCard
      title={game.title}
      description={game.blurb}
      art={live ? 'featured' : 'workshop'}
      status={live ? 'live' : 'preview'}
      meta={
        <span className="arc-meta-item">
          <Icon name="users" size={16} />
          {t('showroom.arcade.players', { players: game.players })}
        </span>
      }
      capabilities={game.tags}
    >
      <div className="arc-actions">
        <Button
          size="sm"
          variant={live ? 'primary' : 'secondary'}
          onClick={() => previewHint(t(live ? 'showroom.hint.play' : 'showroom.hint.notify'))}
        >
          {live ? t('showroom.arcade.play') : t('showroom.arcade.notify')}
        </Button>
      </div>
    </GameCard>
  );
}

/**
 * The Arcade with sample cards (UX-SHOW-001): live builds ("Playable now") apart from showcase cards ("Preview / in
 * development"), in the same hero and card style as the real page (ARC-UI-010). The built-in games are the real
 * catalogue; the rest are fixtures: no `front` request is made and no button reaches the server.
 */
export default function PreviewArcade() {
  const t = useT();
  const live = FIXTURE_GAMES.filter((g) => g.status === 'live');
  const soon = FIXTURE_GAMES.filter((g) => g.status === 'preview');
  const builtins = builtinsByStatus();
  return (
    <main className="arc-page" data-testid="showroom-arcade">
      <ArcadeHero
        links={[
          { href: '#srm-live-games', label: t('showroom.arcade.playableNow') },
          { href: '#srm-soon-games', label: t('showroom.arcade.inDevelopment') },
        ]}
      >
        <PreviewToggle />
      </ArcadeHero>
      <section className="arc-section" aria-labelledby="srm-live" id="srm-live-games">
        <SectionHead id="srm-live" title={t('showroom.arcade.playableNow')} />
        <div className="arc-grid">
          {builtins.live.map((g) => (
            <BuiltinCard key={g.id} game={g} flags={[]} />
          ))}
          {live.map((g) => (
            <SampleCard key={g.id} game={g} />
          ))}
        </div>
      </section>
      <section className="arc-section" aria-labelledby="srm-soon" id="srm-soon-games">
        <SectionHead id="srm-soon" title={t('showroom.arcade.inDevelopment')} />
        <div className="arc-grid">
          {builtins.preview.map((g) => (
            <BuiltinCard key={g.id} game={g} flags={[]} />
          ))}
          {soon.map((g) => (
            <SampleCard key={g.id} game={g} />
          ))}
        </div>
      </section>
    </main>
  );
}
