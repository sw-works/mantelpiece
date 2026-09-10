import { useState } from "react";
import { Pressable, Text } from "react-native";
import { theme } from "../theme";

/** D-pad friendly button: focus ring, large hit target, no gestures required. */
export function FocusButton({
  label,
  onPress,
  preferred = false,
  testID,
}: {
  label: string;
  onPress: () => void;
  preferred?: boolean;
  testID?: string;
}) {
  const [focused, setFocused] = useState(false);
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      hasTVPreferredFocus={preferred}
      onFocus={() => setFocused(true)}
      onBlur={() => setFocused(false)}
      onPress={onPress}
      style={{
        paddingVertical: 18,
        paddingHorizontal: 32,
        borderRadius: theme.radius,
        backgroundColor: focused ? theme.accent : theme.panel,
        borderWidth: 3,
        borderColor: focused ? theme.focus : "transparent",
      }}
    >
      <Text style={{ color: focused ? "#111" : theme.fg, fontSize: theme.body, fontWeight: "600" }}>{label}</Text>
    </Pressable>
  );
}
