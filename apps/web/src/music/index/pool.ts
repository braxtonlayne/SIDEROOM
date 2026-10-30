// A bounded worker pool: at most `concurrency` items in flight (MUS-IDX-001: the tagger runs 4 at a time).

export const TAG_CONCURRENCY = 4;

export interface PoolOptions {
  concurrency?: number;
  signal?: AbortSignal;
}

/**
 * Run `worker` over `items`, at most `concurrency` at once, in order of start. A worker that throws does not stop
 * the others (the worker is expected to handle its own failures); the first unexpected error is rethrown at the
 * end. Stops taking new items once `signal` aborts.
 */
export async function runPool<T>(
  items: readonly T[],
  worker: (item: T, index: number) => Promise<void>,
  o: PoolOptions = {},
): Promise<void> {
  const limit = Math.max(1, o.concurrency ?? TAG_CONCURRENCY);
  let next = 0;
  let firstError: unknown;
  let failed = false;
  const lane = async () => {
    while (next < items.length && !o.signal?.aborted) {
      const i = next++;
      try {
        await worker(items[i] as T, i);
      } catch (e) {
        if (!failed) {
          failed = true;
          firstError = e;
        }
      }
    }
  };
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, lane));
  if (failed) throw firstError;
}
