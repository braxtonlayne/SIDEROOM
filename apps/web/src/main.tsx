import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { RouterProvider } from '@tanstack/react-router';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { VoiceBridge } from './lounge/voice/VoiceBridge';
import { createAppRouter } from './router';
import './styles/fonts.css';
import './styles/tokens.css';

const queryClient = new QueryClient();
const router = createAppRouter();

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
      {/* Outside the router so a voice call survives leaving the Lounge (VOX-CLI-010). */}
      <VoiceBridge />
    </QueryClientProvider>
  </StrictMode>,
);
