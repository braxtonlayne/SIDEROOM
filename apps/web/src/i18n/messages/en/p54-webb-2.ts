// SEC-AUD-040 (wave 54): words the public pages need at first paint. A CORE fragment (registered by
// `../en.ts` with entrance, a11y and pwa), so every key here must stay free of app section and theme names
// (D-385): the public surfaces load it.
export default {
  // Form checks on the sign-in pages (register, forgot, reset).
  'entrance.validation.email': 'Enter a valid email address.',
  'entrance.validation.username':
    '2-32 characters: lowercase letters, numbers, dots and underscores.',
  'entrance.validation.passwordShort': 'Use at least {min} characters.',
  'entrance.validation.passwordWeak': 'That password is too easy to guess. Mix it up.',
  // Words the @sideroom/ui primitives speak on their own (UiStringsProvider, components/AppUiStrings.tsx).
  'a11y.kit.close': 'Close',
  'a11y.kit.confirm': 'Confirm',
  'a11y.kit.cancel': 'Cancel',
  'a11y.kit.typeToConfirm': 'Type "{text}" to confirm',
  'a11y.kit.showPassword': 'Show password',
  'a11y.kit.hidePassword': 'Hide password',
  'a11y.kit.show': 'Show',
  'a11y.kit.hide': 'Hide',
  'a11y.kit.passwordStrength': 'Password strength',
  'a11y.kit.strength0': 'Too weak',
  'a11y.kit.strength1': 'Weak',
  'a11y.kit.strength2': 'Fair',
  'a11y.kit.strength3': 'Good',
  'a11y.kit.strength4': 'Strong',
  'a11y.kit.errorTitle': 'Something went wrong',
  'a11y.kit.errorDescription': 'An unexpected error stopped this page from loading.',
  'a11y.kit.technicalDetails': 'Technical details',
  'a11y.kit.tryAgain': 'Try again',
  'a11y.kit.noResults': 'No results',
  'a11y.kit.removeOption': 'Remove {label}',
  'a11y.kit.selectedCount': '{count, number} selected',
  'a11y.kit.loading': 'Loading',
  'a11y.kit.submenu': 'Submenu',
} satisfies Record<string, string>;
