import { render } from "@testing-library/react";
import type { ReactElement } from "react";
import { describe, expect, it } from "vitest";
import { axeViolations } from "../test-axe";
import { ThemeProvider } from "../theme/ThemeProvider";
import * as Kit from "./Kit.stories";
import * as Misc from "./Misc.stories";
import * as Primitives from "./Primitives.stories";

// Q72: every story is rendered inside the theme provider and must pass axe (contrast off, see test-axe.ts).
const stories = Object.fromEntries(
  Object.entries({ ...Kit, ...Misc, ...Primitives }).filter((entry): entry is [string, () => ReactElement] => typeof entry[1] === "function"),
);

describe("stories (Q72)", () => {
  it("exports a story for every primitive group", () => {
    expect(Object.keys(stories).length).toBeGreaterThanOrEqual(25);
  });

  it.each(Object.entries(stories))("%s has no axe violations", async (_name, Story) => {
    render(
      <ThemeProvider section="lounge">
        <Story />
      </ThemeProvider>,
    );
    // Portalled content (modals, toasts, menus) lives in document.body, so check the whole body.
    expect(await axeViolations(document.body)).toEqual([]);
  });
});
