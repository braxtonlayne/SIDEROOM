import { afterEach, describe, expect, it, vi } from 'vitest';

async function freshStore() {
  vi.resetModules();
  return import('./session');
}

afterEach(() => {
  localStorage.clear();
});

describe('FU-ENT-4: /app reopens the last section', () => {
  it('remembers a section and reads it back after a reload of the store', async () => {
    const a = await freshStore();
    expect(a.useUiStore.getState().lastSection).toBe('/channels');
    a.rememberSection('/study');
    const b = await freshStore();
    expect(b.useUiStore.getState().lastSection).toBe('/study');
  });

  it('remembers a channel path but not settings, auth, error or query-string pages', async () => {
    const a = await freshStore();
    a.rememberSection('/channels/12/34');
    expect(a.useUiStore.getState().lastSection).toBe('/channels/12/34');
    for (const p of [
      '/settings/profile',
      '/login',
      '/not-a-page',
      '/admin',
      '/study?x=1',
      '/app',
    ]) {
      a.rememberSection(p);
    }
    expect(a.useUiStore.getState().lastSection).toBe('/channels/12/34');
  });

  it('remembers the emulator page (FU-16)', async () => {
    const a = await freshStore();
    a.rememberSection('/games/emulator');
    expect(a.useUiStore.getState().lastSection).toBe('/games/emulator');
  });

  it('ignores a damaged stored value', async () => {
    localStorage.setItem('sr.lastSection.v1', '/settings/account');
    const a = await freshStore();
    expect(a.useUiStore.getState().lastSection).toBe('/channels');
  });

  it('forgets the section on sign-out, so the next account starts at the default', async () => {
    const a = await freshStore();
    a.rememberSection('/study');
    a.forgetSection();
    expect(a.useUiStore.getState().lastSection).toBe('/channels');
    const b = await freshStore();
    expect(b.useUiStore.getState().lastSection).toBe('/channels');
  });
});
