import { useState } from "react";
import { Icon } from "../icons/Icon";
import { ICON_NAMES } from "../icons/paths";
import { Avatar } from "../primitives/Avatar/Avatar";
import { AvatarStack } from "../primitives/Avatar/AvatarStack";
import { Chip, Pill, Tag } from "../primitives/Badge/Chip";
import { Button } from "../primitives/Button/Button";
import { ColorPicker } from "../primitives/ColorPicker/ColorPicker";
import { DatePicker, TimePicker } from "../primitives/DateTime/DateTime";
import { Accordion, Collapsible } from "../primitives/Disclosure/Disclosure";
import { Drawer } from "../primitives/Drawer/Drawer";
import { FileDrop } from "../primitives/FileDrop/FileDrop";
import { KbdCombo } from "../primitives/Kbd/Kbd";
import { ProgressBar } from "../primitives/Progress/ProgressBar";
import { SettingsLayout } from "../primitives/Settings/SettingsLayout";
import { SegmentedControl, Slider } from "../primitives/Slider/Slider";
import { EmptyState, ErrorBoundary, ErrorPage } from "../primitives/State/State";
import { ThemeProvider } from "../theme/ThemeProvider";
import { DoorLogo, SectionTopBar, SrFooter, SrHeader } from "../sideroom/Chrome";
import { DropdownChip, FilterChips } from "../sideroom/FilterChips";
import { PosterCard, PosterRow } from "../sideroom/Poster";

export default { title: "Primitives" };

export const SliderWithMarks = () => (
  <div style={{ maxWidth: 360 }}>
    <Slider label="Volume" defaultValue={40} showValue formatValue={(v) => `${v}%`} step={10} marks={[{ value: 0, label: "0" }, { value: 50, label: "50" }, { value: 100, label: "100" }]} />
  </div>
);

export const Segmented = () => (
  <SegmentedControl label="Message density" options={[{ value: "cozy", label: "Cozy" }, { value: "compact", label: "Compact" }, { value: "x", label: "Disabled", disabled: true }]} />
);

export const DrawerStory = () => {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button onClick={() => setOpen(true)}>Open drawer</Button>
      <Drawer open={open} onClose={() => setOpen(false)} title="Members" footer={<Button onClick={() => setOpen(false)}>Done</Button>}>
        <p>Drawer body</p>
      </Drawer>
    </>
  );
};

export const Disclosure = () => (
  <div style={{ display: "grid", gap: 24, maxWidth: 420 }}>
    <Collapsible variant="category" title="Text channels" defaultOpen>
      # general
    </Collapsible>
    <Accordion items={[{ id: "a", title: "What is Side Room?", content: "A private place." }, { id: "b", title: "Who can join?", content: "Invited members." }]} defaultOpen={["a"]} />
  </div>
);

export const Avatars = () => (
  <div style={{ display: "flex", gap: 24, alignItems: "center" }}>
    <AvatarStack items={["Ada", "Grace", "Linus", "Margaret", "Ken", "Dennis"].map((name) => ({ name }))} max={4} />
    <Avatar name="Ada Lovelace" size={56} status="online" decoration={<svg viewBox="0 0 40 40"><circle cx="20" cy="20" r="19" fill="none" stroke="gold" strokeWidth="2" /></svg>} />
  </div>
);

export const PillsTagsChips = () => (
  <div style={{ display: "flex", gap: 12, alignItems: "center" }}>
    <Pill tone="accent">Live</Pill>
    <Tag>Fantasy</Tag>
    <Tag variant="filled">TAG</Tag>
    <Chip selected>Selected</Chip>
    <Chip>Idle</Chip>
  </div>
);

export const Progress = () => (
  <div style={{ display: "grid", gap: 16, maxWidth: 320 }}>
    <ProgressBar label="Upload" value={65} />
    <ProgressBar label="Working" />
    <ProgressBar label="Round" variant="circular" value={65} />
    <ProgressBar label="Round indeterminate" variant="circular" />
  </div>
);

export const EmptyAndError = () => (
  <>
    <EmptyState icon={<Icon name="users" size={24} />} title="No friends online" description="When friends come online they will show up here." action={<Button>Add friend</Button>} />
    <ErrorBoundary
      fallback={({ error, reset }) => <ErrorPage error={error} action={<Button onClick={reset}>Retry</Button>} />}
    >
      <ErrorPage title="Page not found" description="That room does not exist." />
    </ErrorBoundary>
  </>
);

export const KbdStory = () => <KbdCombo keys={["Ctrl", "/"]} />;
export const Color = () => <ColorPicker label="Server accent" />;
export const DateAndTime = () => (
  <div style={{ display: "flex", gap: 16 }}>
    <DatePicker label="Date" />
    <TimePicker label="Time" hint="Local time" />
  </div>
);
export const FileDropStory = () => <FileDrop label="Upload cover art" accept="image/*" maxSize={2_000_000} hint="PNG, JPG or WebP up to 2 MB" onFiles={() => {}} />;

export const IconSet = () => (
  <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(110px, 1fr))", gap: 16 }}>
    {ICON_NAMES.map((n) => (
      <div key={n} style={{ display: "grid", justifyItems: "center", gap: 6, fontSize: 11 }}>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <Icon name={n} size={24} />
          <Icon name={n} size={20} />
          <Icon name={n} size={16} />
        </div>
        {n}
      </div>
    ))}
  </div>
);

export const Settings = () => {
  const [active, setActive] = useState("acct");
  const [dirty, setDirty] = useState(true);
  return (
    <SettingsLayout
      inline
      title="User settings"
      sections={[{ id: "acct", label: "My Account", group: "User settings" }, { id: "look", label: "Appearance", group: "User settings" }, { id: "out", label: "Log out", danger: true }]}
      activeId={active}
      onSelect={setActive}
      onClose={() => {}}
      dirty={dirty}
      onSave={() => setDirty(false)}
      onReset={() => setDirty(false)}
    >
      <h1>{active}</h1>
    </SettingsLayout>
  );
};

const NAV = [
  { id: "lounge", label: "The lounge", href: "#lounge" },
  { id: "cinema", label: "The cinema", href: "#cinema" },
  { id: "library", label: "The library", href: "#library" },
];

export const SideRoomChrome = () => (
  <ThemeProvider section="library">
    <SectionTopBar items={NAV} activeId="library" voiceSlot={<span>In voice</span>} />
    <SrHeader tagline="Good things stay between us." action={<a href="#login">Member access</a>} />
    <DoorLogo size={64} label="Side Room" />
    <SrFooter links={[{ label: "About", href: "#about" }, { label: "Privacy", href: "#privacy" }]} />
  </ThemeProvider>
);

export const PostersAndFilters = () => (
  <ThemeProvider section="library" style={{ padding: 24 }}>
    <FilterChips label="Genre" options={["Action", "Fantasy", "Romance"].map((g) => ({ value: g, label: g }))} />
    <FilterChips multiple label="Tags" options={["One", "Two"].map((g) => ({ value: g, label: g }))} />
    <DropdownChip label="Room selection" options={[{ value: "1", label: "Room selection" }, { value: "2", label: "Back room" }]} />
    <PosterRow title="Trending this week" exploreHref="#all">
      {["Witch Hat Atelier", "Dandadan", "Sakamoto Days", "Chainsaw Man"].map((t, i) => (
        <PosterCard key={t} title={t} subtitle="Manga · Ch. 86" meta="Fantasy · Adventure" rating="★ 9.2" tag={i === 0 ? "#1 Trending" : undefined} progress={i === 1 ? 45 : undefined} href="#p" actions={<Button iconOnly aria-label={`Add ${t} to list`}><Icon name="plus" size={16} /></Button>} />
      ))}
    </PosterRow>
  </ThemeProvider>
);
