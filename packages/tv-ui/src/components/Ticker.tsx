import { Text, View } from "react-native";
import { theme } from "../theme";

/**
 * F-160: the live line. One pill, top right of Memories home, with an amber dot for a
 * connected feed: "Maya added 42 photos from day 3".
 */
export function Ticker({ line }: { line: string | null }) {
  if (!line) return null;
  return (
    <View
      testID="ticker"
      style={{
        position: "absolute",
        right: theme.safe.x,
        top: theme.safe.y,
        flexDirection: "row",
        alignItems: "center",
        gap: 12,
        height: 48,
        paddingLeft: 14,
        paddingRight: 20,
        borderRadius: 24,
        backgroundColor: "#0b0b0fd9",
        borderWidth: 1,
        borderColor: "#ffffff1f",
      }}
    >
      <View style={{ width: 10, height: 10, borderRadius: 5, backgroundColor: theme.accent }} />
      <Text numberOfLines={1} style={{ color: theme.fg, fontSize: theme.eyebrow }}>
        {line}
      </Text>
    </View>
  );
}
