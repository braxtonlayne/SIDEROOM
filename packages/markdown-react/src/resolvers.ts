import type { ReactNode } from "react";
import type { LinkKind } from "./ast";

export interface ResolvedName {
  name: string;
  /** CSS color for role/user colouring. */
  color?: string;
}

/**
 * Everything the renderer cannot know from the AST alone. All members are optional;
 * unresolved entities render as neutral placeholders.
 */
export interface Resolvers {
  user?: (id: string) => ResolvedName | undefined;
  role?: (id: string) => ResolvedName | undefined;
  channel?: (id: string) => ResolvedName | undefined;
  /** Image URL for a custom emoji. Without it the emoji renders as `:name:`. */
  customEmojiUrl?: (id: string, animated: boolean) => string | undefined;
  /**
   * Replaces the whole rendering of a custom emoji (the web app draws a clickable emoji with a popout). When it
   * returns a node, `customEmojiUrl` is not used.
   */
  customEmoji?: (emoji: { id: string; name: string; animated: boolean }) => ReactNode;
  /** Rich card for `<cinema:id>`. Without it a plain chip is shown. */
  cinema?: (id: string) => ReactNode;
  /** Rich card for `<library:id>`. Without it a plain chip is shown. */
  library?: (id: string) => ReactNode;
  /** Display name for `</name:id>` is the AST name; this can add an affordance. */
  slashCommand?: (name: string, id: string) => ReactNode;
}

export interface LinkInfo {
  url: string;
  kind: LinkKind;
  /** Plain-text label as displayed. */
  label: string;
}

export interface MarkdownHooks {
  /**
   * Called before following a *masked* link (`[text](url)`), whose label can hide the real
   * destination. Resolve to `true` to open it. When omitted, masked links behave like
   * normal anchors. Auto links never call this.
   */
  confirmLink?: (link: LinkInfo) => boolean | Promise<boolean>;
  /** Called when a mention chip is clicked (profile popout, channel jump, ...). */
  onMentionClick?: (target: { kind: "user" | "role" | "channel"; id: string }) => void;
  /** Syntax highlighting seam for code blocks (e.g. a lazy-loaded highlighter). */
  renderCodeBlock?: (block: { lang: string | null; code: string }) => ReactNode;
}
