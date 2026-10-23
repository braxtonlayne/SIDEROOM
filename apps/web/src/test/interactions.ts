import { act, waitFor } from '@testing-library/react';
import { onTestFinished } from 'vitest';
import { interactionConfig } from '../lounge/msg/Components';
import { emitDispatch } from '../store/events';
import { http, HttpResponse, server } from './server';

/**
 * Holds a component interaction open until the test ends it. A short `interactionConfig.timeoutMs`
 * races the test: under load, `userEvent` can take longer than the wait before the busy state is
 * asserted, so the control is already idle again. Here the wait is long, the `POST /interactions`
 * nonce is captured, and `fail()` delivers `INTERACTION_FAILURE` for it, the same path the gateway
 * takes, so "This interaction failed" appears only when the test says so.
 */
export function holdInteractions(): { fail: () => Promise<void> } {
  const before = interactionConfig.timeoutMs;
  interactionConfig.timeoutMs = 60_000;
  onTestFinished(() => {
    interactionConfig.timeoutMs = before;
  });
  const nonces: string[] = [];
  server.use(
    http.post('*/api/v1/interactions', async ({ request }) => {
      const body = (await request.json()) as { nonce?: string };
      if (body.nonce) nonces.push(body.nonce);
      return new HttpResponse(null, { status: 204 });
    }),
  );
  return {
    fail: async () => {
      await waitFor(() => expect(nonces.length).toBeGreaterThan(0));
      act(() => {
        for (const nonce of nonces.splice(0)) emitDispatch('INTERACTION_FAILURE', { nonce });
      });
    },
  };
}
