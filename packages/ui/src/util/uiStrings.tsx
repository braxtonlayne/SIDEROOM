import { createContext, useContext, useMemo, type ReactNode } from "react";

/**
 * Words the primitives speak on their own (screen-reader prefixes, built-in buttons). The library ships English
 * defaults; an app passes its translated strings once, near its root, with `<UiStringsProvider>`. Props on a
 * single component still win over these.
 */
export interface UiStrings {
  /** Screen-reader prefix of a Notice, read before its title ("Information: ..."). */
  noticeInfo: string;
  noticeSuccess: string;
  noticeWarning: string;
  noticeError: string;
  /** The × button of a dismissible Notice or toast. */
  dismiss: string;
  /** Name of the toast live region. */
  notifications: string;
  /** ColorPicker: the hex field's label and the eyedropper button. */
  colorHex: string;
  colorPick: string;
  /** SEC-AUD-040: the × of a Modal or Drawer. */
  close: string;
  /** ConfirmDialog's default buttons and the "type this to confirm" field. */
  confirm: string;
  cancel: string;
  typeToConfirm: (text: string) => string;
  /** PasswordInput: the show/hide toggle and the strength meter (five levels, weakest first). */
  showPassword: string;
  hidePassword: string;
  show: string;
  hide: string;
  passwordStrength: string;
  strengthLevels: readonly [string, string, string, string, string];
  /** ErrorState's defaults. */
  errorTitle: string;
  errorDescription: string;
  technicalDetails: string;
  tryAgain: string;
  /** Select: no match, a chip's remove button, the count of chosen options. */
  noResults: string;
  removeOption: (label: string) => string;
  selectedCount: (count: number) => string;
  /** A Spinner's (and a loading Button's) accessible name. */
  loading: string;
  /** A submenu whose label is not plain text. */
  submenu: string;
}

export const UI_STRINGS_EN: UiStrings = {
  noticeInfo: "Information",
  noticeSuccess: "Success",
  noticeWarning: "Warning",
  noticeError: "Error",
  dismiss: "Dismiss",
  notifications: "Notifications",
  colorHex: "Hex",
  colorPick: "Pick from screen",
  close: "Close",
  confirm: "Confirm",
  cancel: "Cancel",
  typeToConfirm: (text) => `Type "${text}" to confirm`,
  showPassword: "Show password",
  hidePassword: "Hide password",
  show: "Show",
  hide: "Hide",
  passwordStrength: "Password strength",
  strengthLevels: ["Too weak", "Weak", "Fair", "Good", "Strong"],
  errorTitle: "Something went wrong",
  errorDescription: "An unexpected error stopped this page from loading.",
  technicalDetails: "Technical details",
  tryAgain: "Try again",
  noResults: "No results",
  removeOption: (label) => `Remove ${label}`,
  selectedCount: (count) => `${count} selected`,
  loading: "Loading",
  submenu: "Submenu",
};

const UiStringsContext = createContext<UiStrings>(UI_STRINGS_EN);

export function UiStringsProvider({ value, children }: { value: Partial<UiStrings>; children: ReactNode }) {
  const merged = useMemo(() => ({ ...UI_STRINGS_EN, ...value }), [value]);
  return <UiStringsContext.Provider value={merged}>{children}</UiStringsContext.Provider>;
}

export function useUiStrings(): UiStrings {
  return useContext(UiStringsContext);
}
