import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PosterRow } from "../sideroom/Poster";
import { UiStringsProvider } from "../util/uiStrings";
import { Notice } from "./Feedback/Feedback";
import { SettingsLayout } from "./Settings/SettingsLayout";

// The words the primitives speak on their own can be translated by the app (XC-VIS-002); English stays the default.
describe("translated built-in words", () => {
  it("Notice takes its screen-reader prefix and Dismiss from UiStringsProvider, props win", () => {
    render(
      <UiStringsProvider value={{ noticeWarning: "Achtung", dismiss: "Schließen" }}>
        <Notice variant="warning" onDismiss={() => {}}>
          Low disk
        </Notice>
        <Notice variant="success" variantLabel="Erfolg">
          Saved
        </Notice>
        <Notice variant="info">Plain</Notice>
      </UiStringsProvider>,
    );
    expect(screen.getByText("Achtung:", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("Erfolg:", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("Information:", { exact: false })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Schließen" })).toBeInTheDocument();
  });

  it("PosterRow arrows take labels, English by default", () => {
    const { rerender } = render(
      <PosterRow title="Row">
        <span>x</span>
      </PosterRow>,
    );
    expect(screen.getByRole("button", { name: "Scroll back" })).toBeInTheDocument();
    rerender(
      <PosterRow title="Row" prevLabel="Zurück" nextLabel="Weiter">
        <span>x</span>
      </PosterRow>,
    );
    expect(screen.getByRole("button", { name: "Zurück" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Weiter" })).toBeInTheDocument();
  });

  it("SettingsLayout close button and unsaved bar take labels", () => {
    render(
      <SettingsLayout
        inline
        title="Einstellungen"
        sections={[{ id: "a", label: "A" }]}
        activeId="a"
        onSelect={() => {}}
        onClose={() => {}}
        dirty
        labels={{ close: "Schließen", closeTarget: "Einstellungen", unsaved: "Ungespeichert!", reset: "Zurücksetzen", save: "Speichern" }}
      >
        <p>Body</p>
      </SettingsLayout>,
    );
    expect(screen.getByRole("button", { name: /Schließen.*Einstellungen/ })).toBeInTheDocument();
    expect(screen.getByText("Ungespeichert!")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Zurücksetzen" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Speichern" })).toBeInTheDocument();
  });
});
