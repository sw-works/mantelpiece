// 10-foot UI: photo-first, near-black chrome, amber for focus and anything live.
// The design canvas this follows: MemoryLane Fire TV (2026-09-16).

export const theme = {
  bg: "#0b0b0f",
  panel: "#16161c",
  panel2: "#1f1f27",
  fg: "#f2f2f5",
  muted: "#9a9aa5",
  dim: "#5c5c68",
  accent: "#ffb454",
  /** Text on an amber surface. */
  onAccent: "#14100a",
  ok: "#7ed49a",
  bad: "#ff7a7a",
  focus: "#ffffff",
  radius: 16,
  gap: 24,
  /** Type ramp. Nothing below `credit` on a TV. */
  display: 96,
  title: 56,
  heading: 30,
  body: 26,
  small: 22,
  credit: 19,
  eyebrow: 20,
  /**
   * TV safe zone at 1920x1080. Screens keep text inside the 5% title-safe margin;
   * interactive elements may reach the 3.5% action-safe margin.
   */
  safe: { x: 90, y: 54 },
  actionSafe: { x: 67, y: 38 },
  /** Left navigation rail; content starts at rail + 40. */
  rail: 300,
} as const;

/** Left edge of screen content beside the rail. */
export const CONTENT_X = theme.rail + 40;
