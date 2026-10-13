import { LoungeShell } from '../lounge/LoungeShell';
import { MockDataProvider } from '../lounge/mockData';

/** The Lounge layout: gateway-backed data provider around the shell. Loaded lazily by lounge.tsx. */
export function LoungeRoot() {
  return (
    <MockDataProvider>
      <LoungeShell />
    </MockDataProvider>
  );
}
