import { Text, View } from "react-native";
import { type LandmarkFacts } from "../api/client";
import { theme } from "../theme";

/**
 * F-210: what a landmark in the photos is. The name, the encyclopedia's one-line
 * description, one or two trivia lines, and the credit the text is shown under.
 */
export function LandmarkCard({ facts, lines = 2, width = 620, overlay = false, testID }: { facts: LandmarkFacts; lines?: number; width?: number; overlay?: boolean; testID?: string }) {
  return (
    <View
      testID={testID ?? `landmark-${facts.key}`}
      style={{
        width,
        gap: 8,
        padding: overlay ? 24 : 0,
        borderRadius: overlay ? 20 : 0,
        backgroundColor: overlay ? "#0b0b0fd9" : "transparent",
        borderWidth: overlay ? 1 : 0,
        borderColor: "#ffffff1f",
      }}
    >
      <Text style={{ color: theme.accent, fontSize: theme.eyebrow, fontWeight: "700", letterSpacing: 2 }}>{facts.title.toUpperCase()}</Text>
      {facts.tagline && <Text style={{ color: theme.muted, fontSize: theme.small }}>{facts.tagline}</Text>}
      {facts.lines.slice(0, lines).map((line, i) => (
        <Text key={i} style={{ color: theme.fg, fontSize: overlay ? 26 : theme.small, lineHeight: overlay ? 36 : 30 }}>
          {line}
        </Text>
      ))}
      <Text style={{ color: theme.dim, fontSize: theme.credit }}>{`From ${facts.credit}`}</Text>
    </View>
  );
}
