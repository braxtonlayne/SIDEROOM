import type { ToolProps } from '../registry';
import TimerPanel from './TimerPanel';

export default function StopwatchTool(props: ToolProps) {
  return <TimerPanel kind="stopwatch" {...props} />;
}
