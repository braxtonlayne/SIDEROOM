# Self-hosted fonts

Latin-subset WOFF2 files from the Fontsource npm packages (version 5.3.0), all under the SIL Open
Font License 1.1 (see the `OFL-*.txt` files). `packages/ui/src/theme/fonts.ts` lists the expected names.

| File | Family | Package |
| --- | --- | --- |
| `newsreader-variable.woff2`, `newsreader-italic-variable.woff2` | Newsreader | `@fontsource-variable/newsreader` (wght) |
| `hanken-grotesk-variable.woff2` | Hanken Grotesk | `@fontsource-variable/hanken-grotesk` (wght) |
| `ibm-plex-mono-400.woff2`, `ibm-plex-mono-500.woff2` | IBM Plex Mono | `@fontsource/ibm-plex-mono` |
| `inter-variable.woff2` | Inter (open-licensed sans-serif face) | `@fontsource-variable/inter` (wght) |

Only the Latin subset ships. Add latin-ext, cyrillic or other subsets with `unicode-range` faces if the
community needs them.
