import type { ToolProps } from '../registry';
import { NarrowStudio } from './StudioTool';

/** Palette Extractor: the studio with only the palette panel. */
export default function PaletteTool(_props: ToolProps) {
  return <NarrowStudio modes={['palette']} initial="palette" />;
}
