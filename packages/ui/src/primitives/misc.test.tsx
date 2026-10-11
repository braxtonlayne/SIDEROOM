import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { axeViolations } from "../test-axe";
import { ICON_NAMES } from "../icons/paths";
import { Icon } from "../icons/Icon";
import { DoorLogo, SectionTopBar, SrFooter, SrHeader } from "../sideroom/Chrome";
import { DropdownChip, FilterChips } from "../sideroom/FilterChips";
import { PosterCard, PosterRow } from "../sideroom/Poster";
import { Avatar } from "./Avatar/Avatar";
import { AvatarStack } from "./Avatar/AvatarStack";
import { Chip, Pill, Tag } from "./Badge/Chip";
import { ColorPicker } from "./ColorPicker/ColorPicker";
import { DatePicker, TimePicker } from "./DateTime/DateTime";
import { Accordion, Collapsible } from "./Disclosure/Disclosure";
import { Drawer } from "./Drawer/Drawer";
import { FileDrop, filterFiles } from "./FileDrop/FileDrop";
import { Kbd, KbdCombo } from "./Kbd/Kbd";
import { ProgressBar } from "./Progress/ProgressBar";
import { SettingsLayout } from "./Settings/SettingsLayout";
import { SegmentedControl, Slider } from "./Slider/Slider";
import { EmptyState, ErrorBoundary, ErrorPage } from "./State/State";

describe("Slider", () => {
  it("is a labelled range, updates, and renders marks", () => {
    const onValueChange = vi.fn();
    render(<Slider label="Volume" defaultValue={20} showValue marks={[{ value: 0, label: "0" }, { value: 100, label: "100" }]} onValueChange={onValueChange} />);
    const s = screen.getByRole("slider", { name: "Volume" });
    expect(s).toHaveValue("20");
    fireEvent.change(s, { target: { value: "60" } });
    expect(onValueChange).toHaveBeenCalledWith(60);
    expect(screen.getByText("60")).toBeInTheDocument();
    expect(screen.getByText("100")).toBeInTheDocument();
  });
});

describe("SegmentedControl", () => {
  it("selects a single option and respects disabled", async () => {
    const onValueChange = vi.fn();
    render(
      <SegmentedControl
        label="Density"
        onValueChange={onValueChange}
        options={[{ value: "a", label: "Cozy" }, { value: "b", label: "Compact" }, { value: "c", label: "Off", disabled: true }]}
      />,
    );
    expect(screen.getByRole("radiogroup", { name: "Density" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Cozy" })).toBeChecked();
    await userEvent.click(screen.getByText("Compact"));
    expect(onValueChange).toHaveBeenCalledWith("b");
    expect(screen.getByRole("radio", { name: "Compact" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Off" })).toBeDisabled();
  });
});

describe("Drawer", () => {
  it("is a modal dialog that closes on Escape and the close button, and restores focus", async () => {
    const onClose = vi.fn();
    render(
      <>
        <button>opener</button>
        <Drawer open onClose={onClose} title="Members">
          <button>inside</button>
        </Drawer>
      </>,
    );
    const d = screen.getByRole("dialog", { name: "Members" });
    expect(d).toHaveAttribute("aria-modal", "true");
    expect(screen.getByRole("button", { name: "Close" })).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("traps Tab and renders nothing when closed", async () => {
    const { rerender } = render(
      <Drawer open onClose={() => {}} title="T">
        <button>last</button>
      </Drawer>,
    );
    screen.getByRole("button", { name: "last" }).focus();
    await userEvent.tab();
    expect(screen.getByRole("button", { name: "Close" })).toHaveFocus();
    rerender(
      <Drawer open={false} onClose={() => {}} title="T" />,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("Collapsible and Accordion", () => {
  it("toggles aria-expanded", async () => {
    render(<Collapsible title="Text channels">content</Collapsible>);
    const b = screen.getByRole("button", { name: "Text channels" });
    expect(b).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(b);
    expect(b).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("region", { name: "Text channels" })).toBeVisible();
  });

  it("keeps a single panel open unless multiple", async () => {
    render(
      <Accordion
        items={[
          { id: "1", title: "One", content: "c1" },
          { id: "2", title: "Two", content: "c2" },
        ]}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "One" }));
    await userEvent.click(screen.getByRole("button", { name: "Two" }));
    expect(screen.getByRole("button", { name: "One" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("button", { name: "Two" })).toHaveAttribute("aria-expanded", "true");
  });
});

describe("Avatar decoration and AvatarStack", () => {
  it("renders an aria-hidden decoration", () => {
    const { container } = render(<Avatar name="Ada Lovelace" decoration={<svg data-testid="deco" />} />);
    expect(screen.getByTestId("deco").closest("[aria-hidden]")).not.toBeNull();
    expect(container.querySelector("[role=img][aria-label='Ada Lovelace']")).not.toBeNull();
  });

  it("collapses overflow into +N with an accessible name", () => {
    const items = ["A", "B", "C", "D", "E", "F"].map((name) => ({ name }));
    render(<AvatarStack items={items} max={3} label="In the room" />);
    expect(screen.getByRole("group", { name: "In the room" })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "and 3 more" })).toBeInTheDocument();
  });
});

describe("Pill, Tag, Chip", () => {
  it("Chip exposes toggle state and fires onClick", async () => {
    const onClick = vi.fn();
    render(<Chip selected onClick={onClick}>Action</Chip>);
    const c = screen.getByRole("button", { name: "Action" });
    expect(c).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(c);
    expect(onClick).toHaveBeenCalled();
    render(<><Pill>New</Pill><Tag>Fantasy</Tag></>);
    expect(screen.getByText("Fantasy")).toBeInTheDocument();
  });
});

describe("ProgressBar", () => {
  it("reports value and clamps", () => {
    render(<ProgressBar label="Upload" value={150} max={100} />);
    const p = screen.getByRole("progressbar", { name: "Upload" });
    expect(p).toHaveAttribute("aria-valuenow", "100");
  });
  it("is indeterminate without a value, circular too", () => {
    render(<><ProgressBar label="Linear" /><ProgressBar label="Round" variant="circular" value={25} /></>);
    expect(screen.getByRole("progressbar", { name: "Linear" })).not.toHaveAttribute("aria-valuenow");
    expect(screen.getByRole("progressbar", { name: "Round" })).toHaveAttribute("aria-valuenow", "25");
  });
});

describe("EmptyState and ErrorBoundary", () => {
  it("shows empty copy and action", () => {
    render(<EmptyState title="No friends yet" description="Add some." action={<button>Add</button>} />);
    expect(screen.getByText("No friends yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add" })).toBeInTheDocument();
  });

  it("catches render errors, shows the error page, and retries", async () => {
    let boom = true;
    function Bomb() {
      if (boom) throw new Error("kaboom");
      return <p>recovered</p>;
    }
    const onError = vi.fn();
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <ErrorBoundary onError={onError}>
        <Bomb />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Something went wrong");
    expect(screen.getByText("kaboom")).toBeInTheDocument();
    expect(onError).toHaveBeenCalled();
    boom = false;
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(screen.getByText("recovered")).toBeInTheDocument();
    spy.mockRestore();
  });
});

describe("Kbd", () => {
  it("combo has a readable name", () => {
    render(<KbdCombo keys={["Ctrl", "K"]} />);
    expect(screen.getByLabelText("Ctrl plus K")).toBeInTheDocument();
    render(<Kbd>Esc</Kbd>);
  });
});

describe("ColorPicker", () => {
  it("picks swatches and validates hex input", async () => {
    const onValueChange = vi.fn();
    render(<ColorPicker label="Accent" defaultValue="#c2b08f" swatches={["#ff0000", "#00ff00"]} onValueChange={onValueChange} />);
    await userEvent.click(screen.getByRole("button", { name: "#ff0000" }));
    expect(onValueChange).toHaveBeenLastCalledWith("#ff0000");
    expect(screen.getByRole("button", { name: "#ff0000" })).toHaveAttribute("aria-pressed", "true");
    const hex = screen.getByLabelText("Hex");
    await userEvent.clear(hex);
    await userEvent.type(hex, "#0af");
    expect(onValueChange).toHaveBeenLastCalledWith("#00aaff");
    await userEvent.clear(hex);
    await userEvent.type(hex, "zzz");
    expect(hex).toHaveAttribute("aria-invalid", "true");
  });
});

describe("DatePicker / TimePicker", () => {
  it("are labelled native inputs that report values", () => {
    const onValueChange = vi.fn();
    render(<><DatePicker label="Date" onValueChange={onValueChange} /><TimePicker label="Time" error="Required" /></>);
    const d = screen.getByLabelText("Date");
    expect(d).toHaveAttribute("type", "date");
    fireEvent.change(d, { target: { value: "2026-10-11" } });
    expect(onValueChange).toHaveBeenCalledWith("2026-10-11");
    expect(screen.getByLabelText("Time")).toHaveAttribute("aria-invalid", "true");
  });
});

describe("FileDrop", () => {
  const file = (name: string, type: string, size = 10) => new File([new Uint8Array(size)], name, { type });

  it("filterFiles enforces type, size and count", () => {
    const r = filterFiles([file("a.png", "image/png"), file("b.txt", "text/plain"), file("c.png", "image/png", 999), file("d.png", "image/png"), file("e.png", "image/png")], {
      accept: "image/*",
      maxSize: 100,
      multiple: true,
      maxFiles: 2,
    });
    expect(r.accepted.map((f) => f.name)).toEqual(["a.png", "d.png"]);
    expect(r.rejected.map((x) => `${x.file.name}:${x.reason}`)).toEqual(["b.txt:type", "c.png:size", "e.png:count"]);
  });

  it("accepts dropped files and reports rejects", () => {
    const onFiles = vi.fn();
    const onReject = vi.fn();
    const { container } = render(<FileDrop label="Upload cover" accept=".png" onFiles={onFiles} onReject={onReject} />);
    const zone = container.firstElementChild!;
    fireEvent.drop(zone, { dataTransfer: { files: [file("x.png", "image/png"), file("y.gif", "image/gif")] } });
    expect(onFiles).toHaveBeenCalledWith([expect.objectContaining({ name: "x.png" })]);
    expect(onReject).toHaveBeenCalledWith([expect.objectContaining({ reason: "type" })]);
    expect(screen.getByRole("button", { name: "Upload cover" })).toBeInTheDocument();
  });
});

describe("Icon", () => {
  it("is decorative by default and labelled on request", () => {
    const { container } = render(<><Icon name="search" /><Icon name="bell" label="Notifications" size={16} /></>);
    expect(container.querySelector("svg[data-icon=search]")).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByRole("img", { name: "Notifications" })).toHaveAttribute("width", "16");
  });
  it("every icon has a non-empty path", () => {
    expect(ICON_NAMES.length).toBeGreaterThan(50);
    for (const n of ICON_NAMES) {
      const { container, unmount } = render(<Icon name={n} />);
      expect(container.querySelector("path")?.getAttribute("d")?.length).toBeGreaterThan(3);
      unmount();
    }
  });
});

describe("SettingsLayout", () => {
  const sections = [
    { id: "acct", label: "My Account", group: "User" },
    { id: "look", label: "Appearance", group: "User" },
    { id: "out", label: "Log out", danger: true },
  ];
  it("navigates sections and closes on Escape when clean", async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    render(<SettingsLayout inline title="User settings" sections={sections} activeId="acct" onSelect={onSelect} onClose={onClose}>body</SettingsLayout>);
    expect(screen.getByRole("button", { name: "My Account" })).toHaveAttribute("aria-current", "page");
    await userEvent.click(screen.getByRole("button", { name: "Appearance" }));
    expect(onSelect).toHaveBeenCalledWith("look");
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
  });
  it("warns instead of closing when dirty; save and reset fire", async () => {
    const onClose = vi.fn();
    const onSave = vi.fn();
    const onReset = vi.fn();
    render(<SettingsLayout inline dirty title="User settings" sections={sections} activeId="acct" onSelect={() => {}} onClose={onClose} onSave={onSave} onReset={onReset}>body</SettingsLayout>);
    expect(screen.getByRole("alert")).toHaveTextContent("unsaved changes");
    await userEvent.keyboard("{Escape}");
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveAttribute("data-nudge");
    await userEvent.click(screen.getByRole("button", { name: "Save changes" }));
    await userEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(onSave).toHaveBeenCalled();
    expect(onReset).toHaveBeenCalled();
  });
});

describe("Side Room chrome", () => {
  it("header, footer, top bar", () => {
    render(
      <>
        <SrHeader tagline="Good things stay between us." action={<a href="/login">Member access</a>} />
        <SectionTopBar
          activeId="library"
          voiceSlot={<span>In voice</span>}
          items={[
            { id: "lounge", label: "The lounge", href: "/lounge" },
            { id: "cinema", label: "The cinema", href: "/cinema" },
            { id: "library", label: "The library", href: "/library" },
          ]}
        />
        <SrFooter links={[{ label: "About", href: "/about" }]} />
        <DoorLogo label="Side Room" />
      </>,
    );
    expect(screen.getByRole("banner")).toHaveTextContent("SIDE ROOM");
    expect(screen.getByRole("link", { name: "The library" })).toHaveAttribute("aria-current", "page");
    expect(screen.queryByText(/board/i)).toBeNull();
    expect(screen.getByRole("contentinfo")).toHaveTextContent("Not everywhere. Just here.");
    expect(screen.getByRole("img", { name: "Side Room" })).toBeInTheDocument();
  });

  it("PosterCard shows tag, progress and actions; PosterRow pages", async () => {
    render(
      <PosterRow title="Trending this week" exploreHref="/all">
        <PosterCard title="Witch Hat Atelier" subtitle="Manga · Ch. 86" tag="#1 Trending" progress={40} href="/m/1" actions={<button aria-label="Add to list">+</button>} />
      </PosterRow>,
    );
    expect(screen.getByRole("heading", { name: "Trending this week" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Explore all" })).toHaveAttribute("href", "/all");
    expect(screen.getByRole("link", { name: "Witch Hat Atelier" })).toHaveAttribute("href", "/m/1");
    expect(screen.getByRole("progressbar", { name: "Witch Hat Atelier progress" })).toHaveAttribute("aria-valuenow", "40");
    expect(screen.getByText("#1 Trending")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Add to list" }));
    await userEvent.click(screen.getByRole("button", { name: "Scroll forward" }));
  });

  it("FilterChips single clears on re-click, multi accumulates", async () => {
    const single = vi.fn();
    const multi = vi.fn();
    render(
      <>
        <FilterChips label="Genre" options={[{ value: "a", label: "Action" }, { value: "b", label: "Fantasy" }]} onValueChange={single} />
        <FilterChips multiple label="Tags" options={[{ value: "x", label: "X" }, { value: "y", label: "Y" }]} onValueChange={multi} />
      </>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Action" }));
    expect(single).toHaveBeenLastCalledWith("a");
    await userEvent.click(screen.getByRole("button", { name: "Action" }));
    expect(single).toHaveBeenLastCalledWith(null);
    await userEvent.click(screen.getByRole("button", { name: "X" }));
    await userEvent.click(screen.getByRole("button", { name: "Y" }));
    expect(multi).toHaveBeenLastCalledWith(["x", "y"]);
  });

  it("DropdownChip is a labelled select", async () => {
    const onValueChange = vi.fn();
    render(<DropdownChip label="Room selection" options={[{ value: "1", label: "Lounge" }, { value: "2", label: "Cinema" }]} onValueChange={onValueChange} />);
    await userEvent.selectOptions(screen.getByLabelText("Room selection"), "2");
    expect(onValueChange).toHaveBeenCalledWith("2");
  });
});

describe("axe", () => {
  it("has no violations across the new primitives", async () => {
    const { container } = render(
      <div>
        <Slider label="Volume" defaultValue={30} marks={[{ value: 0, label: "0" }]} />
        <SegmentedControl label="Density" options={[{ value: "a", label: "A" }, { value: "b", label: "B" }]} />
        <Collapsible title="Category" defaultOpen>
          body
        </Collapsible>
        <Accordion items={[{ id: "1", title: "Q", content: "A" }]} />
        <AvatarStack items={[{ name: "A" }, { name: "B" }, { name: "C" }]} max={2} />
        <Avatar name="Ada" decoration={<span />} />
        <Chip selected>Chip</Chip>
        <Pill>Pill</Pill>
        <Tag>Tag</Tag>
        <ProgressBar label="Linear" value={30} />
        <ProgressBar label="Circular" variant="circular" />
        <EmptyState title="Nothing" />
        <ErrorPage />
        <KbdCombo keys={["Ctrl", "K"]} />
        <ColorPicker label="Accent" />
        <DatePicker label="Date" />
        <TimePicker label="Time" />
        <FileDrop label="Upload" hint="PNG only" onFiles={() => {}} />
        <Icon name="search" label="Search" />
        <SrHeader tagline="Tagline" action={<a href="/x">Member access</a>} />
        <SectionTopBar items={[{ id: "a", label: "The lounge", href: "/l" }]} activeId="a" />
        <SrFooter />
        <PosterRow title="Row">
          <PosterCard title="Title" subtitle="Sub" progress={10} href="/p" />
        </PosterRow>
        <FilterChips label="Genre" options={[{ value: "a", label: "Action" }]} />
        <DropdownChip label="Rooms" options={[{ value: "1", label: "One" }]} />
        <SettingsLayout inline dirty title="Settings" sections={[{ id: "a", label: "Account" }]} activeId="a" onSelect={() => {}} onClose={() => {}}>
          body
        </SettingsLayout>
      </div>,
    );
    expect(await axeViolations(container)).toEqual([]);
  });

  it("an open drawer has no violations", async () => {
    render(
      <Drawer open onClose={() => {}} title="Members">
        <p>list</p>
      </Drawer>,
    );
    expect(await axeViolations(document.body)).toEqual([]);
  });
});
