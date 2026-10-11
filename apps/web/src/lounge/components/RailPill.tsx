/**
 * Missing from @sideroom/ui: the Discord-style indicator pill on the left edge of a rail item.
 * `unread` = small dot, `hover` = medium, `selected` = tall (LNG-RAIL-004).
 */
export type RailPillState = 'none' | 'unread' | 'hover' | 'selected';

export function RailPill({ state }: { state: RailPillState }) {
  return <span className="lng-pill" data-state={state} aria-hidden="true" />;
}
