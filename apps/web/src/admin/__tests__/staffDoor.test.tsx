import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('../../app/NotFoundPage', () => ({ NotFoundPage: () => <h1>nothing here</h1> }));
import { StaffDoor } from '../StaffDoor';

const Page = () => <h1>admin page</h1>;

describe('StaffDoor (SEC-AUD-172)', () => {
  it('a non-staff answer shows not-found and never calls the importer', async () => {
    const load = vi.fn(() => Promise.resolve(Page));
    render(<StaffDoor ask={() => Promise.reject(new Error('403'))} load={load} />);
    expect(await screen.findByText('nothing here')).toBeTruthy();
    expect(load).not.toHaveBeenCalled();
  });

  it('a staff answer imports and renders the page', async () => {
    const load = vi.fn(() => Promise.resolve(Page));
    render(<StaffDoor ask={() => Promise.resolve({})} load={load} />);
    expect(await screen.findByText('admin page')).toBeTruthy();
    expect(load).toHaveBeenCalledTimes(1);
  });

  it('a refused import renders not-found, not a crash', async () => {
    render(
      <StaffDoor ask={() => Promise.resolve({})} load={() => Promise.reject(new Error('404'))} />,
    );
    await waitFor(() => expect(screen.getByText('nothing here')).toBeTruthy());
  });

  it('shows the quiet fallback while it waits', () => {
    render(<StaffDoor ask={() => new Promise(() => {})} load={() => Promise.resolve(Page)} />);
    expect(screen.getByRole('status')).toBeTruthy();
  });
});

describe('StaffDoor and the staff catalog (SEC-AUD-187, D-663)', () => {
  it('the Admin app renders only after the staff catalog has registered', async () => {
    let registered: () => void = () => {};
    const loadCatalog = vi.fn(() => new Promise<void>((r) => (registered = r)));
    const load = vi.fn(() => Promise.resolve(Page));
    render(<StaffDoor ask={() => Promise.resolve({})} load={load} loadCatalog={loadCatalog} />);
    await waitFor(() => expect(loadCatalog).toHaveBeenCalledTimes(1));
    // The Admin chunk is in, the catalog is not: nothing of the app is shown yet.
    await waitFor(() => expect(load).toHaveBeenCalledTimes(1));
    expect(screen.queryByText('admin page')).toBeNull();
    registered();
    expect(await screen.findByText('admin page')).toBeTruthy();
  });

  it('a member (the /admin/me question is refused) never asks for the staff catalog', async () => {
    const loadCatalog = vi.fn(() => Promise.resolve());
    render(
      <StaffDoor
        ask={() => Promise.reject(new Error('403'))}
        load={() => Promise.resolve(Page)}
        loadCatalog={loadCatalog}
      />,
    );
    expect(await screen.findByText('nothing here')).toBeTruthy();
    expect(loadCatalog).not.toHaveBeenCalled();
  });

  it('a catalog that cannot load still opens the app (the catalog settles degraded, keys show)', async () => {
    render(
      <StaffDoor
        ask={() => Promise.resolve({})}
        load={() => Promise.resolve(Page)}
        loadCatalog={() => Promise.resolve()}
      />,
    );
    expect(await screen.findByText('admin page')).toBeTruthy();
  });
});
