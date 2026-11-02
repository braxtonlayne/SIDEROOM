// OPS-TDEP-001 (D-450 to D-452, threat model T49): a TEST deployment serves Activities from `/_activity/<id>/` on
// the main host (`[activities] origin_mode = "same_host_sandboxed"`, config or environment only). The server says
// so with the client flag below (`GET /sr/features/flags`); the client then frames Activities on that path and shows
// the "Test deployment" notice. Absent means the default: every Activity on its own origin.
import { create } from 'zustand';

export const TEST_DEPLOYMENT_FLAG = 'deploy.test_same_host';

export const useTestDeployment = create<{ on: boolean }>(() => ({ on: false }));

/**
 * Note a flag map from the server. Sticky: the mode comes from the server's config, which cannot change while it
 * runs, and the gateway's flag updates (`SR_FEATURE_UPDATE`) carry only the feature registry's flags.
 */
export function noteClientFlags(flags: Record<string, boolean>): void {
  if (flags[TEST_DEPLOYMENT_FLAG] === true && !useTestDeployment.getState().on) {
    useTestDeployment.setState({ on: true });
  }
}

/** Whether this server is a test deployment with same-host Activities (read once, outside React). */
export const isTestDeployment = (): boolean => useTestDeployment.getState().on;
