import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { RouterProvider } from '@tanstack/react-router';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { LiveRegions } from './a11y/announcer';
import { initLocale } from './i18n';
import { PwaHost } from './pwa/PwaHost';
import { registerServiceWorker } from './pwa/register';
import { ReportUnavailableDialog, ReportsFlagSync } from './moderation/reports';
import { MiniPlayerMount } from './cinema/player/MiniPlayerMount';
import { IncomingCallOverlay } from './lounge/voice/IncomingCall';
import { VoiceBridge } from './lounge/voice/VoiceBridge';
import { createAppRouter } from './router';
import './styles/fonts.css';
import './styles/tokens.css';
import './styles/a11y.css';

const queryClient = new QueryClient();
const router = createAppRouter();

// Restore the saved language before first paint of translated text; failures fall back to English.
void initLocale().finally(() => {
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
        <PwaHost />
        <LiveRegions />
        <ReportsFlagSync />
        <ReportUnavailableDialog />
        {/* Outside the router so a voice call survives leaving the Lounge (VOX-CLI-010). */}
        <VoiceBridge />
        <IncomingCallOverlay />
        {/* Same reason: the Cinema mini player keeps playing across routes (CIN-PLY-011). */}
        <MiniPlayerMount history={router.history} />
      </QueryClientProvider>
    </StrictMode>,
  );
  registerServiceWorker();
});
