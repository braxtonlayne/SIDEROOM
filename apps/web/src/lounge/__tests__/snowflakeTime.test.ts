import { describe, expect, it } from 'vitest';
import { SIDEROOM_EPOCH_MS } from '../epoch.generated';
import { snowflakeTime } from '../liveMap';

describe('snowflakeTime', () => {
  it('uses the SIDEROOM epoch, matching sr-core', () => {
    expect(SIDEROOM_EPOCH_MS).toBe(1_767_225_600_000n);
    // id with timestamp bits = 12_345 ms after the epoch (see sr-core snowflake tests)
    const id = (12_345n << 22n) | 7n;
    expect(snowflakeTime(id.toString())).toBe(new Date(1_767_225_600_000 + 12_345).toISOString());
  });
  it('returns an empty string for non-snowflakes', () => {
    expect(snowflakeTime(null)).toBe('');
    expect(snowflakeTime('abc')).toBe('');
  });
});
