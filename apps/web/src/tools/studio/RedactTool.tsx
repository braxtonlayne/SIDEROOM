import { NarrowStudio } from './StudioTool';

/** Redact and Pixelate: the studio with only the redact and export panels. */
export default function RedactTool() {
  return <NarrowStudio modes={['redact', 'export']} initial="redact" />;
}
