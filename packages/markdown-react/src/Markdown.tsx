import { Fragment, useCallback, useState } from "react";
import type { KeyboardEvent, MouseEvent, ReactNode } from "react";
import { nodesToText } from "./ast";
import type { LinkKind, MdNode } from "./ast";
import type { MarkdownHooks, Resolvers } from "./resolvers";
import { CodeBlock } from "./highlight";
import { formatTimestamp, formatTimestampTitle } from "./timestamp";
import type { TimestampFormatOptions } from "./timestamp";
import { useRelativeNow } from "./ticker";

export interface MarkdownProps extends MarkdownHooks, TimestampFormatOptions {
  /** The AST from `sr-markdown` (`JSON.parse(parse_to_json(text))`). */
  nodes: readonly MdNode[];
  /** Result of the Rust `is_jumbo`; the renderer never re-derives it. */
  jumbo?: boolean;
  resolvers?: Resolvers;
  /** Show every spoiler un-hidden (the "always reveal spoilers" setting). */
  revealSpoilers?: boolean;
  className?: string;
}

interface Ctx extends MarkdownHooks, TimestampFormatOptions {
  resolvers: Resolvers;
  revealSpoilers: boolean;
  jumbo: boolean;
}

const SAFE_URL = /^https?:\/\//i;

export function Markdown({
  nodes,
  jumbo = false,
  resolvers = {},
  revealSpoilers = false,
  className,
  ...rest
}: MarkdownProps) {
  const ctx: Ctx = { resolvers, revealSpoilers, jumbo, ...rest };
  const cls = ["sr-md", jumbo ? "sr-md-jumbo" : "", className ?? ""].filter(Boolean).join(" ");
  return (
    <div className={cls} data-jumbo={jumbo ? "true" : undefined}>
      {renderNodes(nodes, ctx)}
    </div>
  );
}

function renderNodes(nodes: readonly MdNode[], ctx: Ctx): ReactNode {
  return nodes.map((n, i) => <Fragment key={i}>{renderNode(n, ctx)}</Fragment>);
}

function renderNode(n: MdNode, ctx: Ctx): ReactNode {
  switch (n.type) {
    case "text":
      return n.text;
    case "line_break":
      return <br />;
    case "bold":
      return <strong>{renderNodes(n.children, ctx)}</strong>;
    case "italic":
      return <em>{renderNodes(n.children, ctx)}</em>;
    case "underline":
      return <u>{renderNodes(n.children, ctx)}</u>;
    case "strike":
      return <s>{renderNodes(n.children, ctx)}</s>;
    case "spoiler":
      return <Spoiler forceReveal={ctx.revealSpoilers}>{renderNodes(n.children, ctx)}</Spoiler>;
    case "inline_code":
      return <code className="sr-md-code">{n.code}</code>;
    case "code_block":
      return ctx.renderCodeBlock ? (
        <Fragment>{ctx.renderCodeBlock({ lang: n.lang, code: n.code })}</Fragment>
      ) : (
        <CodeBlock lang={n.lang} code={n.code} />
      );
    case "quote":
      return <blockquote className="sr-md-quote">{renderNodes(n.children, ctx)}</blockquote>;
    case "heading": {
      const level = Math.min(3, Math.max(1, n.level));
      const Tag = `h${level}` as "h1" | "h2" | "h3";
      return <Tag className="sr-md-heading">{renderNodes(n.children, ctx)}</Tag>;
    }
    case "subtext":
      return <small className="sr-md-subtext">{renderNodes(n.children, ctx)}</small>;
    case "list": {
      const Tag = n.ordered ? "ol" : "ul";
      return (
        <Tag className="sr-md-list" start={n.ordered && n.start != null ? n.start : undefined}>
          {n.items.map((item, i) => (
            <li key={i}>{renderNodes(item, ctx)}</li>
          ))}
        </Tag>
      );
    }
    case "link":
      return (
        <Link
          url={n.url}
          kind={n.kind}
          confirmLink={ctx.confirmLink}
          label={nodesToText(n.children)}
        >
          {renderNodes(n.children, ctx)}
        </Link>
      );
    case "user_mention":
    case "role_mention":
    case "channel_mention":
      return <Mention node={n} ctx={ctx} />;
    case "everyone":
      return <span className="sr-md-mention sr-md-mention-everyone">@everyone</span>;
    case "here":
      return <span className="sr-md-mention sr-md-mention-here">@here</span>;
    case "custom_emoji": {
      const custom = ctx.resolvers.customEmoji?.({
        id: String(n.id),
        name: n.name,
        animated: n.animated,
      });
      if (custom) return <>{custom}</>;
      const url = ctx.resolvers.customEmojiUrl?.(String(n.id), n.animated);
      if (!url) return <span className="sr-md-emoji-fallback">{`:${n.name}:`}</span>;
      return (
        <img
          className="sr-md-emoji sr-md-custom-emoji"
          src={url}
          alt={`:${n.name}:`}
          title={`:${n.name}:`}
          data-animated={n.animated ? "true" : undefined}
          draggable={false}
        />
      );
    }
    // Legacy: the parser no longer emits unicode shortcode nodes (Q89); kept so older
    // ASTs and the web app's type guards still render.
    case "emoji":
      return (
        <span
          className="sr-md-emoji"
          role="img"
          aria-label={n.name.replace(/_/g, " ")}
          title={`:${n.name}:`}
        >
          {n.emoji}
        </span>
      );
    case "timestamp":
      return <Timestamp unix={n.unix} style={n.style} ctx={ctx} />;
    case "slash_command": {
      const custom = ctx.resolvers.slashCommand?.(n.name, String(n.id));
      return custom ?? <span className="sr-md-mention sr-md-command">{`/${n.name}`}</span>;
    }
    case "guild_nav":
      return <span className="sr-md-mention sr-md-guild-nav">{navLabel(n.kind)}</span>;
    case "cinema":
      return (
        ctx.resolvers.cinema?.(n.id) ?? (
          <span className="sr-md-mention sr-md-cinema" data-id={n.id}>
            Cinema title
          </span>
        )
      );
    case "library":
      return (
        ctx.resolvers.library?.(n.id) ?? (
          <span className="sr-md-mention sr-md-library" data-id={n.id}>
            Library series
          </span>
        )
      );
    default:
      // Unknown node type from a newer parser: render nothing rather than crash.
      return null;
  }
}

function navLabel(kind: string): string {
  switch (kind) {
    case "customize":
      return "Customize";
    case "browse":
      return "Browse Channels";
    case "guide":
      return "Server Guide";
    case "linked-roles":
      return "Linked Roles";
    default:
      return kind;
  }
}

export function Spoiler({
  children,
  forceReveal,
}: {
  children: ReactNode;
  forceReveal?: boolean;
}) {
  const [revealed, setRevealed] = useState(false);
  const shown = revealed || !!forceReveal;
  const reveal = (e: MouseEvent | KeyboardEvent) => {
    e.stopPropagation();
    e.preventDefault();
    setRevealed(true);
  };
  if (shown) {
    return (
      <span className="sr-md-spoiler" data-revealed="true">
        {children}
      </span>
    );
  }
  return (
    <span
      className="sr-md-spoiler"
      data-revealed="false"
      role="button"
      tabIndex={0}
      aria-label="Spoiler, click to reveal"
      onClick={reveal}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") reveal(e);
      }}
    >
      <span aria-hidden="true" className="sr-md-spoiler-content">
        {children}
      </span>
    </span>
  );
}

function Link({
  url,
  kind,
  label,
  confirmLink,
  children,
}: {
  url: string;
  kind: LinkKind;
  label: string;
  confirmLink: MarkdownHooks["confirmLink"];
  children: ReactNode;
}) {
  const onClick = useCallback(
    async (e: MouseEvent<HTMLAnchorElement>) => {
      if (kind !== "masked" || !confirmLink) return;
      // Every activation of a masked link (including modified clicks) goes through the hook.
      e.preventDefault();
      const ok = await confirmLink({ url, kind, label });
      if (ok) window.open(url, "_blank", "noopener,noreferrer");
    },
    [kind, confirmLink, url, label],
  );
  if (!SAFE_URL.test(url)) return <span className="sr-md-link-invalid">{children}</span>;
  return (
    <a
      className="sr-md-link"
      href={url}
      target="_blank"
      rel="noopener noreferrer nofollow ugc"
      data-kind={kind}
      title={kind === "masked" ? url : undefined}
      onClick={onClick}
    >
      {children}
    </a>
  );
}

type MentionNode = Extract<MdNode, { type: "user_mention" | "role_mention" | "channel_mention" }>;

function Mention({ node, ctx }: { node: MentionNode; ctx: Ctx }) {
  const id = String(node.id);
  const kind =
    node.type === "user_mention" ? "user" : node.type === "role_mention" ? "role" : "channel";
  const resolved = ctx.resolvers[kind]?.(id);
  const prefix = kind === "channel" ? "#" : "@";
  const fallback =
    kind === "user" ? "unknown-user" : kind === "role" ? "deleted-role" : "unknown-channel";
  const label = `${prefix}${resolved?.name ?? fallback}`;
  const onMentionClick = ctx.onMentionClick;
  const common = {
    className: `sr-md-mention sr-md-mention-${kind}`,
    "data-id": id,
    "data-unresolved": resolved ? undefined : "true",
    style: resolved?.color ? { color: resolved.color } : undefined,
  };
  return onMentionClick ? (
    <button type="button" {...common} onClick={() => onMentionClick({ kind, id })}>
      {label}
    </button>
  ) : (
    <span {...common}>{label}</span>
  );
}

type TimestampNode = Extract<MdNode, { type: "timestamp" }>;

function Timestamp({
  unix,
  style,
  ctx,
}: {
  unix: number;
  style: TimestampNode["style"];
  ctx: Ctx;
}) {
  const now = useRelativeNow(style === "relative", ctx.now);
  const opts = { locale: ctx.locale, timeZone: ctx.timeZone, now };
  let text: string;
  let title: string;
  let iso: string | undefined;
  try {
    text = formatTimestamp(unix, style, opts);
    title = formatTimestampTitle(unix, opts);
    iso = new Date(unix * 1000).toISOString();
  } catch {
    text = "Invalid Date";
    title = text;
  }
  return (
    <time
      className="sr-md-timestamp"
      dateTime={iso}
      title={title}
      data-style={style ?? "short_date_time"}
    >
      {text}
    </time>
  );
}
