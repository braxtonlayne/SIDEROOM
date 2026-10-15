/**
 * Types for the `sr-markdown` JSON AST (crates/sr-markdown/src/ast.rs).
 * The Rust crate is the source of truth; the shared corpus in
 * crates/sr-markdown/tests/corpus pins the shape.
 *
 * Ids (`u64` in Rust) are serialized as decimal strings, like Snowflakes elsewhere in the
 * API, because 64-bit values exceed JS's 2^53 safe-integer range. `Id` still tolerates a
 * number so older fixtures keep type-checking, but the parser never emits one.
 */

export type Id = string | number;

export type TimestampStyle =
  | 'short_time'
  | 'long_time'
  | 'short_date'
  | 'long_date'
  | 'short_date_time'
  | 'long_date_time'
  | 'relative';

export type LinkKind = 'masked' | 'auto' | 'auto_suppressed';

export type MdNode =
  | { type: 'text'; text: string }
  | { type: 'line_break' }
  | { type: 'bold'; children: MdNode[] }
  | { type: 'italic'; children: MdNode[] }
  | { type: 'underline'; children: MdNode[] }
  | { type: 'strike'; children: MdNode[] }
  | { type: 'spoiler'; children: MdNode[] }
  | { type: 'inline_code'; code: string }
  | { type: 'code_block'; lang: string | null; code: string }
  | { type: 'quote'; children: MdNode[] }
  | { type: 'heading'; level: number; children: MdNode[] }
  | { type: 'subtext'; children: MdNode[] }
  | { type: 'list'; ordered: boolean; start: number | null; items: MdNode[][] }
  | { type: 'link'; url: string; kind: LinkKind; children: MdNode[] }
  | { type: 'user_mention'; id: Id }
  | { type: 'role_mention'; id: Id }
  | { type: 'channel_mention'; id: Id }
  | { type: 'everyone' }
  | { type: 'here' }
  | { type: 'custom_emoji'; name: string; id: Id; animated: boolean }
  | { type: 'emoji'; name: string; emoji: string }
  | { type: 'timestamp'; unix: number; style: TimestampStyle | null }
  | { type: 'slash_command'; name: string; id: Id }
  | { type: 'guild_nav'; kind: string; id: Id | null }
  | { type: 'cinema'; id: string }
  | { type: 'library'; id: string };

/** Parse the output of the wasm `parse_to_json`. Returns `[]` for malformed input. */
export function parseAstJson(json: string): MdNode[] {
  try {
    const v: unknown = JSON.parse(json);
    return Array.isArray(v) ? (v as MdNode[]) : [];
  } catch {
    return [];
  }
}

/** Plain text of a node list (used for link labels and accessible names). */
export function nodesToText(nodes: readonly MdNode[]): string {
  let out = '';
  for (const n of nodes) {
    switch (n.type) {
      case 'text':
        out += n.text;
        break;
      case 'line_break':
        out += '\n';
        break;
      case 'inline_code':
      case 'code_block':
        out += n.code;
        break;
      case 'emoji':
        out += n.emoji;
        break;
      case 'custom_emoji':
        out += `:${n.name}:`;
        break;
      case 'bold':
      case 'italic':
      case 'underline':
      case 'strike':
      case 'spoiler':
      case 'quote':
      case 'heading':
      case 'subtext':
      case 'link':
        out += nodesToText(n.children);
        break;
      default:
        break;
    }
  }
  return out;
}
