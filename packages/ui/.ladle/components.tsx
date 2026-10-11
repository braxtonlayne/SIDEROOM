import type { GlobalProvider } from "@ladle/react";
import { ThemeProvider } from "../src/theme/ThemeProvider";

/** DS-PRIM-016: every story renders inside the real theme provider (Ladle's theme toggle is unused). */
export const Provider: GlobalProvider = ({ children }) => (
  <ThemeProvider section="lounge" style={{ minHeight: "100vh", padding: 24 }}>
    {children}
  </ThemeProvider>
);
