import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { deleteApp, getApp, keys, patchApp } from './api';
import type { AppPatch, DevApp } from './types';

export const useApp = (appId: string) =>
  useQuery({ queryKey: keys.app(appId), queryFn: () => getApp(appId) });

/** PATCH the app and put the server's answer in the cache (no secrets travel through here). */
export function usePatchApp(appId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (patch: AppPatch) => patchApp(appId, patch),
    onSuccess: (app: DevApp) => {
      qc.setQueryData(keys.app(appId), app);
      void qc.invalidateQueries({ queryKey: keys.apps });
    },
  });
}

export function useDeleteApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ appId, password }: { appId: string; password: string }) =>
      deleteApp(appId, password),
    onSuccess: (_v, { appId }) => {
      qc.removeQueries({ queryKey: keys.app(appId) });
      void qc.invalidateQueries({ queryKey: keys.apps });
    },
  });
}
