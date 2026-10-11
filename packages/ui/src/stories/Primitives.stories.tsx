// DS-PRIM-016 (Q72): a story for each primitive not covered by Misc.stories. Every export here
// and in Misc.stories is rendered and axe-checked by stories.test.tsx.
import { useState } from "react";
import { Badge, CounterBadge } from "../primitives/Badge/Badge";
import { Button } from "../primitives/Button/Button";
import { Checkbox, Radio, RadioGroup, Switch } from "../primitives/Controls/Controls";
import { ConfirmDialog, Modal } from "../primitives/Modal/Modal";
import { Field } from "../primitives/Input/Field";
import { PasswordInput, TextArea, TextInput } from "../primitives/Input/TextInput";
import { Notice, ToastProvider } from "../primitives/Feedback/Feedback";
import { Tabs } from "../primitives/Tabs/Tabs";
import { ContextMenu, DropdownMenu } from "../primitives/Menu/Menu";
import { Popover, Tooltip } from "../primitives/Overlay/Overlay";
import { Combobox, MultiSelect, Select } from "../primitives/Select/Select";
import { ScrollArea, VirtualList } from "../primitives/Scroll/ScrollArea";
import { Skeleton, Spinner } from "../primitives/Loading/Spinner";
import { ErrorBoundary } from "../primitives/State/State";
import { ArrowButton, ArrowLink, DisplayHeading, Eyebrow, GoldButton, OutlineButton } from "../sideroom/Editorial";
import { SrFooter, SrHeader } from "../sideroom/Chrome";

const FRUIT = [
  { value: "apple", label: "Apple" },
  { value: "cherry", label: "Cherry" },
  { value: "plum", label: "Plum", description: "Stone fruit" },
];

export const ButtonVariants = () => (
  <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
    <Button>Primary</Button>
    <Button variant="secondary">Secondary</Button>
    <Button variant="danger">Danger</Button>
    <Button variant="link">Link</Button>
    <Button variant="ghost">Ghost</Button>
    <Button loading>Saving</Button>
  </div>
);

export const Inputs = () => (
  <div style={{ display: "grid", gap: 12, maxWidth: 360 }}>
    <TextInput label="Display name" hint="Shown to others" />
    <TextInput label="Handle" error="Already taken" />
    <TextArea label="Bio" />
    <PasswordInput label="Password" />
    <Field label="Region" hint="Pick the closest" render={(props) => <select {...props}><option>EU</option><option>US</option></select>} />
  </div>
);

export const Toggles = () => {
  const [v, setV] = useState("b");
  return (
    <div style={{ display: "grid", gap: 12 }}>
      <Checkbox label="Remember me" />
      <Checkbox label="Select all" indeterminate />
      <Switch label="Notifications" />
      <RadioGroup legend="Density" value={v} onValueChange={setV}>
        <Radio label="Compact" value="a" />
        <Radio label="Comfortable" value="b" />
      </RadioGroup>
    </div>
  );
};

export const BadgesAndSpinners = () => (
  <div style={{ display: "flex", gap: 12, alignItems: "center", flexWrap: "wrap" }}>
    <Badge variant="accent">New</Badge>
    <Badge variant="danger" pill>Live</Badge>
    <CounterBadge count={12} />
    <Spinner />
    <Skeleton width={160} />
  </div>
);

export const Overlays = () => (
  <div style={{ display: "flex", gap: 12, flexWrap: "wrap" }}>
    <Tooltip content="Copy link">
      <Button variant="secondary">Hover me</Button>
    </Tooltip>
    <Popover trigger={<Button variant="secondary">Details</Button>} label="Details">
      <p>Popover body</p>
    </Popover>
    <DropdownMenu trigger={<Button variant="secondary">Actions</Button>} label="Actions" items={[{ label: "Edit" }, { label: "Share", items: [{ label: "Copy" }] }]} />
    <ContextMenu label="Message actions" items={[{ label: "Reply" }]}>
      <p>Right-click or long-press this line</p>
    </ContextMenu>
  </div>
);

export const DialogsAndNotices = () => {
  const [open, setOpen] = useState(true);
  const [confirm, setConfirm] = useState(false);
  return (
    <div style={{ display: "grid", gap: 12 }}>
      <Notice variant="warning" title="Maintenance tonight">Expect short downtime.</Notice>
      <Notice variant="success" title="Saved">Your changes are live.</Notice>
      <Button onClick={() => setOpen(true)}>Open modal</Button>
      <Button variant="danger" onClick={() => setConfirm(true)}>Delete room</Button>
      <Modal open={open} onClose={() => setOpen(false)} title="Edit profile" description="Changes apply to everyone.">
        <p>Modal body</p>
      </Modal>
      <ConfirmDialog
        open={confirm}
        title="Delete room?"
        description="This cannot be undone."
        danger
        onConfirm={() => setConfirm(false)}
        onCancel={() => setConfirm(false)}
      />
    </div>
  );
};

export const ToastsProvided = () => (
  <ToastProvider>
    <p>Toasts render above this line when triggered.</p>
  </ToastProvider>
);

export const SelectsAndTabs = () => (
  <div style={{ display: "grid", gap: 12, maxWidth: 320 }}>
    <Select aria-label="Fruit" options={FRUIT} placeholder="Pick one" />
    <Combobox aria-label="Fruit search" options={FRUIT} placeholder="Search" />
    <MultiSelect aria-label="Fruits" options={FRUIT} values={["apple"]} />
    <Tabs label="Sections" items={[{ id: "a", label: "Overview", content: "Overview panel" }, { id: "b", label: "Members", content: "Members panel" }]} />
  </div>
);

export const ScrollAndVirtual = () => (
  <div style={{ display: "grid", gap: 12 }}>
    <ScrollArea aria-label="Scrollable notes" style={{ maxHeight: 80 }}>
      <p>Line one</p>
      <p>Line two</p>
      <p>Line three</p>
    </ScrollArea>
    <VirtualList aria-label="Rows" count={100} estimateSize={28} renderItem={(i) => <p>Row {i + 1}</p>} style={{ height: 120 }} />
  </div>
);

export const EditorialKit = () => (
  <div style={{ display: "grid", gap: 12 }}>
    <Eyebrow>Side Room</Eyebrow>
    <DisplayHeading>Not everywhere.</DisplayHeading>
    <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
      <GoldButton>Enter</GoldButton>
      <OutlineButton>Learn more</OutlineButton>
      <ArrowButton>Next</ArrowButton>
    </div>
    <ArrowLink href="#">Browse the library</ArrowLink>
  </div>
);

export const HeaderAndFooter = () => (
  <div>
    <SrHeader />
    <SrFooter links={[{ label: "About", href: "#" }]} />
  </div>
);

function Boom(): never {
  throw new Error("story error");
}

export const ErrorBoundaryFallback = () => {
  // React logs caught errors; the boundary renders its fallback, which is what the story shows.
  const orig = console.error;
  console.error = () => {};
  try {
    return (
      <ErrorBoundary>
        <Boom />
      </ErrorBoundary>
    );
  } finally {
    console.error = orig;
  }
};
