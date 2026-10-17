import { useCallback, useEffect, useRef, useState } from 'react';
import { problemLines } from './api';

export interface Loaded<T> {
  data: T | undefined;
  error: string | null;
  loading: boolean;
  reload: () => void;
  set: (next: T) => void;
}

/** Fetch on mount and when `key` changes; a stale answer for an earlier key is dropped. */
export function useLoad<T>(key: string, fetcher: () => Promise<T>): Loaded<T> {
  const [data, setData] = useState<T | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);
  const fetchRef = useRef(fetcher);
  fetchRef.current = fetcher;
  useEffect(() => {
    let live = true;
    setLoading(true);
    fetchRef.current().then(
      (d) => {
        if (!live) return;
        setData(d);
        setError(null);
        setLoading(false);
      },
      (e: unknown) => {
        if (!live) return;
        setError(problemLines(e).join(' ') || 'Something went wrong.');
        setLoading(false);
      },
    );
    return () => {
      live = false;
    };
  }, [key, tick]);
  const reload = useCallback(() => setTick((n) => n + 1), []);
  return { data, error, loading, reload, set: setData };
}
