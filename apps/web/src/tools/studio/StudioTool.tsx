import type { ToolProps } from '../registry';
import { ImageLoader } from './ImageLoader';
import { StudioEditor, type StudioMode } from './StudioEditor';

/** The full studio, as the drawer's "Image Studio" tool. */
export default function StudioTool(_props: ToolProps) {
  return <ImageLoader>{(bitmap, name) => <StudioEditor bitmap={bitmap} baseName={name} />}</ImageLoader>;
}

/** A studio narrowed to some modes: the Redact and Palette tools are this with different arguments. */
export function NarrowStudio({ modes, initial }: { modes: readonly StudioMode[]; initial: StudioMode }) {
  return (
    <ImageLoader>
      {(bitmap, name) => <StudioEditor bitmap={bitmap} baseName={name} modes={modes} initialMode={initial} />}
    </ImageLoader>
  );
}
