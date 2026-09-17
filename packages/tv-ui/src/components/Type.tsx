import { Text, type TextStyle } from "react-native";
import { theme } from "../theme";

/** Small amber label over a title: "NEW MEMORY · ORGANISED LAST NIGHT". */
export function Eyebrow({ children, testID }: { children: string; testID?: string }) {
  return (
    <Text testID={testID} style={{ color: theme.accent, fontSize: theme.eyebrow, fontWeight: "700", letterSpacing: 2.5, textTransform: "uppercase" }}>
      {children}
    </Text>
  );
}

/** Screen or hero title. */
export function Title({ children, size = theme.title, testID, style }: { children: string; size?: number; testID?: string; style?: TextStyle }) {
  return (
    <Text testID={testID} style={[{ color: theme.fg, fontSize: size, fontWeight: "700", lineHeight: size * 1.08, letterSpacing: -0.5 }, style]}>
      {children}
    </Text>
  );
}

/** Row heading: "On this day", "Day 2 · Fushimi Inari". */
export function Heading({ children, testID }: { children: string; testID?: string }) {
  return (
    <Text testID={testID} style={{ color: theme.fg, fontSize: theme.heading, fontWeight: "600" }}>
      {children}
    </Text>
  );
}

/** Muted meta line. */
export function Meta({ children, size = theme.small, color = theme.muted, testID, style }: { children: string; size?: number; color?: string; testID?: string; style?: TextStyle }) {
  return (
    <Text testID={testID} style={[{ color, fontSize: size, lineHeight: size * 1.4 }, style]}>
      {children}
    </Text>
  );
}
