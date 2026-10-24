import { createMemoryHistory } from '@tanstack/react-router';
import { createAppRouter, parseSearchValue } from '../router';

// A snowflake in the address (`?submission=<id>`, G-68) must survive the router exactly: the default search
// parser made it a JS number and rounded the last digits (the p22 CI run opened …891780 for …891776).
describe('router search parsing', () => {
  it('keeps integers beyond the safe range as strings', () => {
    expect(parseSearchValue('107586859906891776')).toBe('107586859906891776');
    expect(parseSearchValue('-9007199254740993')).toBe('-9007199254740993');
  });

  it('parses everything else like the default', () => {
    expect(parseSearchValue('42')).toBe(42);
    expect(parseSearchValue('true')).toBe(true);
    expect(parseSearchValue('"x"')).toBe('x');
    expect(() => parseSearchValue('plain')).toThrow();
  });

  it('round-trips a snowflake through the location', async () => {
    const router = createAppRouter(
      createMemoryHistory({ initialEntries: ['/admin/review?submission=107586859906891776'] }),
    );
    await router.load();
    expect(router.state.location.searchStr).toBe('?submission=107586859906891776');
    expect((router.state.location.search as Record<string, unknown>).submission).toBe(
      '107586859906891776',
    );
  });
});
