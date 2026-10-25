// RDY-A11Y-004: the appearance modes. Each story renders the same sample in the normal setting and in the mode,
// side by side, for every theme family, so Ladle shows what High contrast and Reduced motion change. Every export
// is rendered and axe-checked by stories.test.tsx.
import type { ReactNode } from "react";
import { Avatar } from "../primitives/Avatar/Avatar";
import { Button } from "../primitives/Button/Button";
import { ProgressBar } from "../primitives/Progress/ProgressBar";
import { Spinner } from "../primitives/Loading/Spinner";
import { ThemeProvider } from "../theme/ThemeProvider";
import type { AppearanceSettings } from "../theme/runtime";
import type { Section } from "../tokens/types";

export default { title: "Appearance modes" };

function Sample({ label }: { label: string }) {
  return (
    <section aria-label={label} style={{ display: "grid", gap: 8, padding: 16, background: "var(--ui-bg)", color: "var(--ui-text)" }}>
      <h3 style={{ margin: 0 }}>{label}</h3>
      <p style={{ margin: 0, color: "var(--ui-text-muted)" }}>Muted text: member since 2026.</p>
      <p style={{ margin: 0, color: "var(--ui-text-faint)" }}>Faint text: last seen an hour ago.</p>
      <div style={{ display: "flex", gap: 8, alignItems: "center", border: "1px solid var(--ui-border)", padding: 8 }}>
        <Avatar name="Ada Lovelace" size={32} status="online" />
        <Button>Primary</Button>
        <Button variant="secondary">Secondary</Button>
        <Spinner label="Loading" />
      </div>
      <ProgressBar value={60} label="Upload progress" />
    </section>
  );
}

function Pair({ section, mode, children }: { section: Section; mode: Partial<AppearanceSettings>; children?: ReactNode }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
      <ThemeProvider section={section}>
        <Sample label={`${section}, normal`} />
      </ThemeProvider>
      <ThemeProvider section={section} appearance={mode}>
        <Sample label={`${section}, ${Object.values(mode).join(" ")}`} />
        {children}
      </ThemeProvider>
    </div>
  );
}

export const HighContrastLounge = () => <Pair section="lounge" mode={{ contrast: "high" }} />;
export const HighContrastSideRoom = () => <Pair section="cinema" mode={{ contrast: "high" }} />;
export const HighContrastLiveTv = () => <Pair section="live-tv" mode={{ contrast: "high" }} />;
export const ReducedMotionLounge = () => <Pair section="lounge" mode={{ reducedMotion: "on" }} />;
export const ReducedMotionSideRoom = () => <Pair section="library" mode={{ reducedMotion: "on" }} />;
