import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ActivityConfig } from '../types';
import { APP, http, HttpResponse, openAt, server, serveApp, signedIn } from './helpers';

beforeEach(signedIn);

const base = (over: Partial<ActivityConfig['config']> = {}): ActivityConfig => ({
  enabled: true,
  origin: 'https://500.activities.chat.example.org',
  config: {
    application_id: '500',
    entry_url: null,
    url_mappings: [],
    review_state: 'draft',
    pinned_sha256: null,
    reviewed_sha256: null,
    review_note: null,
    updated_at_ms: 0,
    ...over,
  },
});

describe('application: activities tab (DEV-PORT-003, DEV-ACT-001)', () => {
  it('saves the entry URL and a mapping, then submits for review', async () => {
    const user = userEvent.setup();
    serveApp({ ...APP });
    let current = base();
    const saved: unknown[] = [];
    let submitted = 0;
    server.use(
      http.get('*/api/v1/applications/500/activity', () => HttpResponse.json(current)),
      http.put('*/api/v1/applications/500/activity', async ({ request }) => {
        const body = (await request.json()) as {
          entry_url: string | null;
          url_mappings: { prefix: string; target: string }[];
        };
        saved.push(body);
        current = base({ entry_url: body.entry_url, url_mappings: body.url_mappings });
        return HttpResponse.json(current);
      }),
      http.post('*/api/v1/applications/500/activity/submit', () => {
        submitted += 1;
        current = base({
          entry_url: current.config.entry_url,
          url_mappings: current.config.url_mappings,
          review_state: 'pending',
          pinned_sha256: 'a'.repeat(64),
        });
        return HttpResponse.json(current);
      }),
    );
    openAt('/developers/applications/500?tab=activities');
    expect(await screen.findByText('https://500.activities.chat.example.org')).toBeInTheDocument();
    // Nothing to submit before an entry URL is saved.
    expect(screen.getByRole('button', { name: 'Submit for review' })).toBeDisabled();

    await user.type(screen.getByLabelText('Entry URL'), 'https://app.example.com/index.html');
    await user.click(screen.getByRole('button', { name: 'Add a mapping' }));
    await user.type(screen.getByLabelText('Path'), '/api');
    await user.type(screen.getByLabelText('Target'), 'https://api.example.com');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toEqual({
      entry_url: 'https://app.example.com/index.html',
      url_mappings: [{ prefix: '/api', target: 'https://api.example.com' }],
    });

    const submit = screen.getByRole('button', { name: 'Submit for review' });
    await waitFor(() => expect(submit).toBeEnabled());
    await user.click(submit);
    await waitFor(() => expect(submitted).toBe(1));
    expect(await screen.findByText('Waiting for staff to review it.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Submit again' })).toBeInTheDocument();
  });

  it('says when Activities are off and shows a reviewer note', async () => {
    serveApp({ ...APP });
    server.use(
      http.get('*/api/v1/applications/500/activity', () =>
        HttpResponse.json({
          ...base({
            entry_url: 'https://app.example.com/',
            review_state: 'rejected',
            review_note: 'Too many permissions',
          }),
          enabled: false,
        }),
      ),
    );
    openAt('/developers/applications/500?tab=activities');
    expect(await screen.findByText(/not switched on for this server yet/)).toBeInTheDocument();
    expect(screen.getByText('Reviewer note: Too many permissions')).toBeInTheDocument();
  });

  it('shows the server field error when the settings are refused', async () => {
    const user = userEvent.setup();
    serveApp({ ...APP });
    server.use(
      http.get('*/api/v1/applications/500/activity', () => HttpResponse.json(base())),
      http.put('*/api/v1/applications/500/activity', () =>
        HttpResponse.json(
          {
            code: 50035,
            message: 'Invalid Form Body',
            errors: {
              entry_url: { _errors: [{ code: 'INVALID_VALUE', message: 'Must be an https URL.' }] },
            },
          },
          { status: 400 },
        ),
      ),
    );
    openAt('/developers/applications/500?tab=activities');
    await user.type(await screen.findByLabelText('Entry URL'), 'http://nope.example.com');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Must be an https URL.');
  });
});
