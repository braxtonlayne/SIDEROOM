// DS-PRIM-016: the remaining primitives, so every exported component has a story (coverage.test.ts enforces
// it). Every export is rendered and axe-checked by stories.test.tsx.
import { useState } from "react";
import { Button } from "../primitives/Button/Button";
import { DateTimePicker } from "../primitives/DateTime/DateTime";
import { KeybindRow } from "../primitives/Kbd/KeybindRow";
import { PermissionMatrix, TriStateToggle, type TriState } from "../primitives/Permissions/Permissions";
import { RailPill } from "../primitives/Rail/RailPill";
import { ResizeHandle } from "../primitives/Resize/ResizeHandle";
import { VirtualGrid } from "../primitives/Scroll/ScrollArea";
import { ComingSoon } from "../sideroom/ComingSoon";

export default { title: "Lounge kit" };

export const DateAndTimeTogether = () => <DateTimePicker label="Starts at" hint="Local time" />;

export const KeybindList = () => (
  <dl style={{ display: "grid", gap: 8, margin: 0 }}>
    <KeybindRow action="Quick switcher" keys={["Ctrl", "K"]} />
    <KeybindRow action="Mark channel read" keys={["Esc"]} />
  </dl>
);

export const RailPillStates = () => (
  <div style={{ display: "flex", gap: 16, alignItems: "center" }}>
    <span>none</span>
    <RailPill state="none" />
    <span>unread</span>
    <RailPill state="unread" />
    <span>hover</span>
    <RailPill state="hover" />
    <span>selected</span>
    <RailPill state="selected" />
  </div>
);

export const ResizableWidth = () => {
  const [width, setWidth] = useState(240);
  return (
    <div style={{ display: "flex", alignItems: "stretch", minHeight: 80 }}>
      <div style={{ width, background: "var(--ui-bg-raised)", padding: 8 }}>Panel ({width}px)</div>
      <ResizeHandle value={width} min={160} max={400} onChange={setWidth} label="Resize panel" invert={false} />
    </div>
  );
};

export const TriStateChoice = () => {
  const [value, setValue] = useState<TriState>("neutral");
  return <TriStateToggle label="Send Messages" value={value} onChange={setValue} />;
};

export const PermissionMatrixStory = () => {
  const [values, setValues] = useState<Record<string, TriState>>({ view: "allow" });
  return (
    <PermissionMatrix
      mode="tristate"
      groups={[
        {
          id: "general",
          label: "General",
          items: [
            { id: "view", label: "View Channel", description: "See the channel in the list" },
            { id: "send", label: "Send Messages" },
            { id: "admin", label: "Administrator", danger: true },
          ],
        },
      ]}
      values={values}
      onChange={(id, v) => setValues((cur) => ({ ...cur, [id]: v }))}
      lockedReason={(id) => (id === "admin" ? "Only the owner can change this" : undefined)}
      searchable
      searchLabel="Search permissions"
    />
  );
};

export const PosterGrid = () => (
  <VirtualGrid
    aria-label="Items"
    count={60}
    columns={4}
    rowHeight={56}
    gap={8}
    renderItem={(i) => <p style={{ margin: 0 }}>Item {i + 1}</p>}
    style={{ height: 180 }}
  />
);

export const ComingSoonModal = () => {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button onClick={() => setOpen(true)}>Open placeholder</Button>
      <ComingSoon open={open} onClose={() => setOpen(false)} title="Create server" />
    </>
  );
};
