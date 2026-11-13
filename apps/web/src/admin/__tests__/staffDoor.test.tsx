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
