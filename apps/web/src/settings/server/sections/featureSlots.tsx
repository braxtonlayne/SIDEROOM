// Feature custom-UI slot registry (FND-FEAT-007). A feature module is a switch plus a settings form the server
// describes (SchemaForm); a module that is also an action (grant a role, add a feed, apply a preset) adds a
// panel of its own. Instead of an `if (feature.id === ...)` per panel in the settings screen, each panel
// registers here under its feature id, as a lazily loaded component, and the screen just renders
// <FeatureSlot>. Adding a module's UI is one `registerFeatureSlot` line next to the module.
import { Suspense, lazy, type ComponentType, type LazyExoticComponent } from 'react';
import type { GuildFeature } from '../featuresApi';

/** What every slot component receives. */
export interface FeatureSlotProps {
  guildId: string;
  feature: GuildFeature;
}

type Loader = () => Promise<{ default: ComponentType<FeatureSlotProps> }>;

const slots = new Map<string, LazyExoticComponent<ComponentType<FeatureSlotProps>>>();

/**
 * Register the panel of a feature. The loader runs the first time the feature is shown (so a panel's code
 * stays out of the settings bundle), and a second registration under the same id replaces the first.
 */
export function registerFeatureSlot(featureId: string, loader: Loader): void {
  slots.set(featureId, lazy(loader));
}

/** Whether a feature has a custom panel. */
export function hasFeatureSlot(featureId: string): boolean {
  return slots.has(featureId);
}

/** The feature's custom panel, or nothing when it has none. */
export function FeatureSlot(props: FeatureSlotProps) {
  const Slot = slots.get(props.feature.id);
  if (!Slot) return null;
  return (
    <Suspense fallback={null}>
      <Slot {...props} />
    </Suspense>
  );
}

// ---- the built-in panels ------------------------------------------------------------------------------------
registerFeatureSlot('moderation.temp_roles', () => import('./TempRolesPanel'));
registerFeatureSlot('community.rss', async () => {
  const { RssPanel } = await import('./RssFeeds');
  return { default: ({ guildId }: FeatureSlotProps) => <RssPanel guildId={guildId} /> };
});
registerFeatureSlot('lounge.slowmode_plus', () => import('./SlowmodePanel'));
