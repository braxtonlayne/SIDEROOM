import axe from "axe-core";

/**
 * Runs axe-core against a rendered container and returns violation summaries (empty = pass).
 * jsdom has no layout or computed colour, so color-contrast and region rules are off; contrast
 * of theme tokens is covered by util/contrast tests.
 */
export async function axeViolations(container: Element): Promise<string[]> {
  const results = await axe.run(container, {
    rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
  });
  return results.violations.map((v) => `${v.id}: ${v.help} (${v.nodes.map((n) => n.target.join(" ")).join(", ")})`);
}
