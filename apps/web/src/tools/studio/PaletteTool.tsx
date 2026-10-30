import { NarrowStudio } from './StudioTool';

/** Palette Extractor: the studio with only the palette panel. */
export default function PaletteTool() {
  return <NarrowStudio modes={['palette']} initial="palette" />;
}
