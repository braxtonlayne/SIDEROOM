import type { Plugin } from 'vite';

/**
 * D-385 (owner O-7): a public surface names no section of the app, so the public build gives `@sideroom/ui` a
 * neutral theme table: only the entrance theme ("sideroom") exists, every section resolves to it, and the
 * section-named theme ids, layout tokens and per-theme selectors are cut out of what is bundled. The app build
 * (and the tests) use the package untouched.
 */
export function neutralTheme(): Plugin {
  const at = (id: string, tail: string) =>
    id.replace(/\\/g, '/').includes(`packages/ui/src/${tail}`);
  return {
    name: 'sideroom-neutral-theme',
    enforce: 'pre',
    transform(code, id) {
      const path = id.split('?')[0]!;
      if (at(path, 'tokens/themes.ts')) {
        const start = code.indexOf('export const sideroom: ThemeDefinition');
        const end = code.indexOf('\n};\n', start) + 4;
        if (start < 0 || end < 4)
          this.error('neutralTheme: cannot find the entrance theme in themes.ts');
        return {
          map: null,
          code:
            'import type { ThemeDefinition, ThemeId, Section } from "./types";\n' +
            code.slice(start, end) +
            '\nexport const THEMES = { sideroom } as unknown as Readonly<Record<ThemeId, ThemeDefinition>>;\n' +
            'export const DEFAULT_THEME_BY_SECTION = new Proxy({}, { get: () => "sideroom" }) as Readonly<Record<Section, ThemeId>>;\n',
        };
      }
      if (at(path, 'theme/tokens.generated.ts')) {
        return {
          map: null,
          code: code
            .replace(
              /export const THEME_IDS = \[[^\]]*\] as const;/,
              'export const THEME_IDS = ["sideroom"] as const;',
            )
            .replace(/^\s*"--lounge-[^\n]*\n/gm, ''),
        };
      }
      if (at(path, 'tokens/types.ts'))
        return {
          map: null,
          code: code.replace(
            /export const SECTIONS = \[[^\]]*\] as const;/,
            'export const SECTIONS = ["entrance"] as const;',
          ),
        };
      if (
        /packages\/ui\/src\/.*\.css/.test(
          at(path, '') ? path.replace(/\\/g, '/') : path.replace(/\\/g, '/'),
        )
      )
        return { map: null, code: code.replace(/^[^\n]*data-theme[^\n]*lounge[^\n]*\n/gm, '') };
      return null;
    },
  };
}
