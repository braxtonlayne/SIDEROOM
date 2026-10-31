import js from '@eslint/js';
import globals from 'globals';
import reactHooks from 'eslint-plugin-react-hooks';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  // public/emu/cores holds the fetched emulator cores (untracked, third-party, never linted; deploy/EMULATOR.md).
  { ignores: ['dist', 'public/emu/cores'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    // The service worker is a classic script (see public/sw.js), not an ES module.
    files: ['public/sw.js'],
    languageOptions: {
      sourceType: 'script',
      globals: { ...globals.serviceworker, module: 'writable' },
    },
  },
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: { globals: { ...globals.browser } },
    plugins: { 'react-hooks': reactHooks },
    rules: { ...reactHooks.configs.recommended.rules },
  },
);
