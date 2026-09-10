import { Text, View } from "react-native";
import { theme } from "../theme";

/** F-160: one line at the bottom of every screen with the latest household activity. */
export function Ticker({ line }: { line: string | null }) {
  if (!line) return null;
  return (
    <View testID="ticker" style={{ position: "absolute", left: 0, right: 0, bottom: 0, padding: 16, backgroundColor: "#000a" }}>
      <Text style={{ color: theme.accent, fontSize: theme.small, textAlign: "center" }}>{line}</Text>
    </View>
  );
}
