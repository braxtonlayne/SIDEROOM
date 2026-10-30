import type { ToolProps } from '../registry';
import TimerPanel from './TimerPanel';

export default function PomodoroTool(props: ToolProps) {
  return <TimerPanel kind="pomodoro" {...props} />;
}
