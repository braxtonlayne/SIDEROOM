import { Markdown, type Resolvers } from '@sideroom/markdown-react';
import { parseMarkdown, useMarkdownReady } from '../markdown';

/** Markdown for a string that is not a whole message (embed text, poll text, forwards). */
export function Md({ text, resolvers }: { text: string; resolvers?: Resolvers | undefined }) {
  useMarkdownReady();
  if (!text) return null;
  return <Markdown nodes={parseMarkdown(text)} {...(resolvers ? { resolvers } : {})} />;
}
