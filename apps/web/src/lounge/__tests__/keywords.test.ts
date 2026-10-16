import { hasKeyword, keywordMatcher } from '../keywords';

describe('keywordMatcher', () => {
  it('matches whole words and phrases, ignoring case', () => {
    const re = keywordMatcher(['deploy', 'release  notes', 'c++']);
    expect(hasKeyword(re, 'Time to DEPLOY!')).toBe(true);
    expect(hasKeyword(re, 'see the release notes')).toBe(true);
    expect(hasKeyword(re, 'learning c++ today')).toBe(true);
    expect(hasKeyword(re, 'redeployed')).toBe(false);
    expect(hasKeyword(re, 'nothing here')).toBe(false);
  });
  it('is null without words', () => {
    expect(keywordMatcher([])).toBeNull();
    expect(hasKeyword(null, 'anything')).toBe(false);
  });
});
