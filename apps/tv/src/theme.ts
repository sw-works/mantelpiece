// 10-foot UI: big type, high contrast, generous focus rings.

export const theme = {
  bg: "#0b0b0f",
  panel: "#16161c",
  fg: "#f2f2f5",
  muted: "#9a9aa5",
  accent: "#ffb454",
  focus: "#ffffff",
  radius: 16,
  gap: 24,
  title: 44,
  body: 26,
  small: 20,
  /**
   * TV safe zone at 1920x1080. Screens keep text inside the 5% title-safe margin;
   * interactive elements may reach the 3.5% action-safe margin.
   */
  safe: { x: 90, y: 54 },
  actionSafe: { x: 67, y: 38 },
} as const;
