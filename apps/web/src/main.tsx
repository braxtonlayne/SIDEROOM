import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { RouterProvider } from '@tanstack/react-router';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { initLocale } from './i18n';
import { PwaHost } from './pwa/PwaHost';
import { registerServiceWorker } from './pwa/register';
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
      </QueryClientProvider>
    </StrictMode>,
  );
  registerServiceWorker();
});
