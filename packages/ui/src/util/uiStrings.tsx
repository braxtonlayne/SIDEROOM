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
};

const UiStringsContext = createContext<UiStrings>(UI_STRINGS_EN);

export function UiStringsProvider({ value, children }: { value: Partial<UiStrings>; children: ReactNode }) {
  const merged = useMemo(() => ({ ...UI_STRINGS_EN, ...value }), [value]);
  return <UiStringsContext.Provider value={merged}>{children}</UiStringsContext.Provider>;
}

export function useUiStrings(): UiStrings {
  return useContext(UiStringsContext);
}
