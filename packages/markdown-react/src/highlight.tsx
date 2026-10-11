import { Fragment, useEffect, useState } from "react";
import type { CSSProperties } from "react";

/** One highlighted token: the text plus its colour in each theme (Shiki CSS-variable mode). */
interface Token {
  content: string;
  light?: string;
  dark?: string;
}

type ShikiModule = typeof import("shiki");
type BundledLanguage = import("shiki").BundledLanguage;

let shiki: Promise<ShikiModule> | undefined;

/** Shiki is loaded on first use, in the browser only, and its grammars load per language. */
function loadShiki(): Promise<ShikiModule> {
  shiki ??= import("shiki");
  return shiki;
}

/**
 * Highlight `code` as `lang`. Resolves to `null` for languages Shiki does not know, so the
 * caller keeps the plain text. Never rejects.
 */
export async function highlightCode(code: string, lang: string): Promise<Token[][] | null> {
  try {
    const { codeToTokensWithThemes } = await loadShiki();
    // Unknown languages make Shiki throw, which lands in the catch below.
    const tokens = await codeToTokensWithThemes(code, {
      lang: lang as BundledLanguage,
      themes: { light: "github-light", dark: "github-dark" },
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
      data-highlighted={lines ? "true" : undefined}
    >
      <code className={lang ? `language-${lang}` : undefined}>
        {lines
          ? lines.map((line, i) => (
              <Fragment key={i}>
                {i > 0 ? "\n" : null}
                {line.map((t, j) => (
                  <span
                    key={j}
                    className="sr-md-tok"
                    style={
                      {
                        "--shiki-light": t.light,
                        "--shiki-dark": t.dark,
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
