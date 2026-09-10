import { useEffect, useRef } from "react";
import { Animated } from "react-native";

/**
 * F-708: a slow drift and zoom over each slide. The motion restarts on every new key
 * and is reduced to a still frame while paused, so the paused photo is what the
 * question on the phone or Alexa+ refers to.
 */
export function KenBurns({ slideKey, paused, durationMs, children }: { slideKey: string; paused: boolean; durationMs: number; children: React.ReactNode }) {
  const progress = useRef(new Animated.Value(0)).current;
  useEffect(() => {
    progress.setValue(0);
    if (paused) return;
    const anim = Animated.timing(progress, { toValue: 1, duration: durationMs, useNativeDriver: false });
    anim.start();
    return () => anim.stop();
  }, [slideKey, paused, durationMs, progress]);
  const scale = paused ? 1 : progress.interpolate({ inputRange: [0, 1], outputRange: [1, 1.08] });
  const translateX = paused ? 0 : progress.interpolate({ inputRange: [0, 1], outputRange: [0, -24] });
  return (
    <Animated.View testID="ken-burns" style={{ transform: [{ scale }, { translateX }] }}>
      {children}
    </Animated.View>
  );
}
