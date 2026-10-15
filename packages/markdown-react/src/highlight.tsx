import { Fragment, useEffect, useState } from 'react';
import type { CSSProperties } from 'react';
import type { HighlighterCore } from 'shiki/core';

/** One highlighted token: the text plus its colour in each theme (Shiki CSS-variable mode). */
interface Token {
  content: string;
  light?: string;
  dark?: string;
}

type Grammar = () => Promise<{ default: unknown }>;

/**
 * The languages that get highlighted (LNG-MD-006), each a separate lazy chunk. Shiki's full bundle ships
 * about 700 grammars (twelve megabytes of chunks and an Oniguruma WASM engine); a chat client needs the few
 * dozen people actually paste. Everything else stays plain text. Grammars are MIT/Apache TextMate bundles
 * packaged by Shiki (MIT).
 */
const GRAMMARS: Record<string, Grammar> = {
  bash: () => import('shiki/langs/bash.mjs'),
  c: () => import('shiki/langs/c.mjs'),
  cpp: () => import('shiki/langs/cpp.mjs'),
  csharp: () => import('shiki/langs/csharp.mjs'),
  css: () => import('shiki/langs/css.mjs'),
  diff: () => import('shiki/langs/diff.mjs'),
  dockerfile: () => import('shiki/langs/dockerfile.mjs'),
  go: () => import('shiki/langs/go.mjs'),
  html: () => import('shiki/langs/html.mjs'),
  ini: () => import('shiki/langs/ini.mjs'),
  java: () => import('shiki/langs/java.mjs'),
  javascript: () => import('shiki/langs/javascript.mjs'),
  json: () => import('shiki/langs/json.mjs'),
  jsx: () => import('shiki/langs/jsx.mjs'),
  kotlin: () => import('shiki/langs/kotlin.mjs'),
  lua: () => import('shiki/langs/lua.mjs'),
  markdown: () => import('shiki/langs/markdown.mjs'),
  php: () => import('shiki/langs/php.mjs'),
  powershell: () => import('shiki/langs/powershell.mjs'),
  python: () => import('shiki/langs/python.mjs'),
  ruby: () => import('shiki/langs/ruby.mjs'),
  rust: () => import('shiki/langs/rust.mjs'),
  sql: () => import('shiki/langs/sql.mjs'),
  swift: () => import('shiki/langs/swift.mjs'),
  toml: () => import('shiki/langs/toml.mjs'),
  tsx: () => import('shiki/langs/tsx.mjs'),
  typescript: () => import('shiki/langs/typescript.mjs'),
  xml: () => import('shiki/langs/xml.mjs'),
  yaml: () => import('shiki/langs/yaml.mjs'),
};

const ALIASES: Record<string, string> = {
  js: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  node: 'javascript',
  ts: 'typescript',
  mts: 'typescript',
  py: 'python',
  python3: 'python',
  rs: 'rust',
  sh: 'bash',
  shell: 'bash',
  shellscript: 'bash',
  zsh: 'bash',
  console: 'bash',
  yml: 'yaml',
  md: 'markdown',
  'c++': 'cpp',
  cc: 'cpp',
  'c#': 'csharp',
  cs: 'csharp',
  kt: 'kotlin',
  rb: 'ruby',
  ps1: 'powershell',
  pwsh: 'powershell',
  docker: 'dockerfile',
  golang: 'go',
  htm: 'html',
  svg: 'xml',
  patch: 'diff',
  jsonc: 'json',
  json5: 'json',
  conf: 'ini',
};

/** The grammar name for a fence language (`js`, `C++`, ...), or null when it is not highlighted. */
export function resolveLanguage(lang: string): string | null {
  const key = lang.trim().toLowerCase();
  const name = ALIASES[key] ?? key;
  return name in GRAMMARS ? name : null;
}

let highlighter: Promise<HighlighterCore> | undefined;
const loaded = new Map<string, Promise<void>>();

/** Shiki core with the JavaScript regex engine and the two themes; created on first use, browser only. */
function loadHighlighter(): Promise<HighlighterCore> {
  highlighter ??= Promise.all([
    import('shiki/core'),
    import('shiki/engine/javascript'),
    import('shiki/themes/github-light.mjs'),
    import('shiki/themes/github-dark.mjs'),
  ]).then(([core, engine, light, dark]) =>
    core.createHighlighterCore({
      themes: [light.default, dark.default],
      langs: [],
      engine: engine.createJavaScriptRegexEngine({ forgiving: true }),
    }),
  );
  return highlighter;
}

function loadGrammar(hl: HighlighterCore, name: string): Promise<void> {
  let p = loaded.get(name);
  if (!p) {
    p = GRAMMARS[name]!().then((m) => hl.loadLanguage(m.default as never));
    loaded.set(name, p);
  }
  return p;
}

/**
 * Highlight `code` as `lang`. Resolves to `null` for languages that are not in the list above, so the
 * caller keeps the plain text. Never rejects.
 */
export async function highlightCode(code: string, lang: string): Promise<Token[][] | null> {
  const name = resolveLanguage(lang);
  if (!name) return null;
  try {
    const hl = await loadHighlighter();
    await loadGrammar(hl, name);
    const tokens = hl.codeToTokensWithThemes(code, {
      lang: name,
      themes: { light: 'github-light', dark: 'github-dark' },
    });
    return tokens.map((line) =>
      line.map((t) => ({
        content: t.content,
        light: t.variants.light?.color,
        dark: t.variants.dark?.color,
      })),
    );
  } catch {
    return null;
  }
}

/**
 * Code block with client-side syntax highlighting. The first paint is the plain code, so
 * the text is always present (and identical to the source) even before Shiki loads.
 */
export function CodeBlock({ lang, code }: { lang: string | null; code: string }) {
  const [lines, setLines] = useState<Token[][] | null>(null);

  useEffect(() => {
    if (!lang) return;
    let live = true;
    void highlightCode(code, lang).then((result) => {
      if (live) setLines(result);
    });
    return () => {
      live = false;
      setLines(null);
    };
  }, [code, lang]);

  return (
    <pre
      className="sr-md-codeblock"
      data-lang={lang ?? undefined}
      data-highlighted={lines ? 'true' : undefined}
    >
      <code className={lang ? `language-${lang}` : undefined}>
        {lines
          ? lines.map((line, i) => (
              <Fragment key={i}>
                {i > 0 ? '\n' : null}
                {line.map((t, j) => (
                  <span
                    key={j}
                    className="sr-md-tok"
                    style={
                      {
                        '--shiki-light': t.light,
                        '--shiki-dark': t.dark,
                      } as CSSProperties
                    }
                  >
                    {t.content}
                  </span>
                ))}
              </Fragment>
            ))
          : code}
      </code>
    </pre>
  );
}
