import type { ToolProps } from '../registry';
import TimerPanel from './TimerPanel';

export default function CountdownTool(props: ToolProps) {
  return <TimerPanel kind="countdown" {...props} />;
}
