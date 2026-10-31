// OPS-FLG-001: the operator's switches for the utility drawer. `platform.tools` hides the whole drawer's tools, and
// `platform.tools.<tool>` (hyphens written as underscores) hides one. They are client-visible platform flags
// (`GET /sr/features/flags`); a flag that is absent (before the flags load, or from a server that predates them) does
// not hide anything, only an explicit `false` does. The server refuses the routes of the tools that have any; this is
// the other half, so a switched-off tool is not offered.
import { useClientFlags } from '../moderation/reports';
import { TOOLS, type ToolEntry } from './registry';

export const TOOLS_FLAG = 'platform.tools';

/** The sub-flag id of a tool of the registry (`image-studio` is `platform.tools.image_studio`). */
export const toolFlagId = (toolId: string): string => `${TOOLS_FLAG}.${toolId.replace(/-/g, '_')}`;

/** Whether the tool is on in `flags`. The drawer's own guide has no switch while the drawer has any tool. */
export function toolAllowed(flags: Record<string, boolean>, toolId: string): boolean {
  if (flags[TOOLS_FLAG] === false) return false;
  if (toolId === 'guide') return true;
  return flags[toolFlagId(toolId)] !== false;
}

/** Whether the tool is on now (reactive). */
export function useToolAllowed(toolId: string): boolean {
  return useClientFlags((s) => toolAllowed(s.flags, toolId));
}

/** The tools of the registry that are on now (reactive). */
export function useAllowedTools(): ToolEntry[] {
  const flags = useClientFlags((s) => s.flags);
  return TOOLS.filter((t) => toolAllowed(flags, t.id));
}
