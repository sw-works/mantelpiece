import { useEffect, useRef } from "react";
import { Animated } from "react-native";

/**
 * F-708: a slow drift and zoom over each slide. The motion restarts on every new key.
 * Pausing holds the frame exactly where it is, so nothing jumps on screen and the paused
 * photo is what a question from the phone or Alexa+ refers to; playing again carries on
 * from that point for the time that was left.
 */
export function KenBurns({ slideKey, paused, durationMs, children }: { slideKey: string; paused: boolean; durationMs: number; children: React.ReactNode }) {
  const progress = useRef(new Animated.Value(0)).current;
  const at = useRef(0);
  const shown = useRef(slideKey);
  useEffect(() => {
    const id = progress.addListener(({ value }) => (at.current = value));
    return () => progress.removeListener(id);
  }, [progress]);
  useEffect(() => {
    if (shown.current !== slideKey) {
      shown.current = slideKey;
      at.current = 0;
      progress.setValue(0);
    }
    if (paused) return;
    const anim = Animated.timing(progress, { toValue: 1, duration: Math.max(0, durationMs * (1 - at.current)), useNativeDriver: false });
    anim.start();
    // Stopping leaves the value where it is: that is the held frame.
    return () => anim.stop();
  }, [slideKey, paused, durationMs, progress]);
  const scale = progress.interpolate({ inputRange: [0, 1], outputRange: [1, 1.08] });
  const translateX = progress.interpolate({ inputRange: [0, 1], outputRange: [0, -24] });
  return (
    <Animated.View testID="ken-burns" style={{ transform: [{ scale }, { translateX }] }}>
      {children}
    </Animated.View>
  );
}
