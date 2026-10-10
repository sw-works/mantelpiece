import { useEffect, useRef, useState } from "react";
import { Pressable, Text } from "react-native";
import { theme } from "../theme";

/**
 * `hasTVPreferredFocus` for one render only, when `preferred` turns true. Vega re-applies a
 * true preference on every re-render, which pulled focus back from wherever the remote had
 * moved it (a fading control bar re-rendering the slideshow took focus off the map).
 */
export function usePreferOnce(preferred: boolean): boolean {
  const was = useRef(false);
  const now = preferred && !was.current;
  useEffect(() => {
    was.current = preferred;
  });
  return now;
}

export type ButtonVariant = "solid" | "ghost" | "chip";

/**
 * D-pad friendly button: pill shape, amber fill with a white ring when focused, large hit
 * target, no gestures required. `ghost` is the secondary action, `chip` the compact
 * filter/selection form (`selected` inverts it).
 */
export function FocusButton({
  label,
  onPress,
  preferred = false,
  testID,
  variant = "solid",
  selected = false,
  disabled = false,
  onFocus,
}: {
  label: string;
  onPress: () => void;
  preferred?: boolean;
  testID?: string;
  variant?: ButtonVariant;
  selected?: boolean;
  disabled?: boolean;
  /** Told when this button takes focus, e.g. to wake faded controls. */
  onFocus?: () => void;
}) {
  const [focused, setFocused] = useState(false);
  const prefer = usePreferOnce(preferred);
  const chip = variant === "chip";
  const background = focused ? theme.accent : selected ? theme.fg : variant === "ghost" ? "#ffffff14" : chip ? "#ffffff12" : theme.panel2;
  const color = focused || selected ? theme.onAccent : theme.fg;
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityState={{ disabled, selected }}
      hasTVPreferredFocus={prefer}
      onFocus={() => {
        setFocused(true);
        onFocus?.();
      }}
      onBlur={() => setFocused(false)}
      onPress={disabled ? undefined : onPress}
      style={{
        height: chip ? 52 : 64,
        paddingHorizontal: chip ? 22 : 30,
        borderRadius: 32,
        justifyContent: "center",
        alignItems: "center",
        backgroundColor: background,
        borderWidth: 4,
        borderColor: focused ? theme.focus : variant === "ghost" && !selected ? "#ffffff22" : "transparent",
        opacity: disabled ? 0.5 : 1,
        alignSelf: "flex-start",
      }}
    >
      <Text style={{ color, fontSize: chip ? 24 : theme.body, fontWeight: chip ? "500" : "600" }}>{label}</Text>
    </Pressable>
  );
}
