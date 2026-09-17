import type { ReactNode } from "react";
import { useWindowDimensions, View } from "react-native";
import { theme } from "../theme";

export const STAGE = { width: 1920, height: 1080 } as const;

/**
 * Every screen is laid out on a 1920x1080 canvas, the size of the design and of the
 * picture on a 1080p set. Fire TV reports its window in points, 960x540 on a 1080p
 * device, so the canvas is scaled to fit; on a 4K set it scales the other way.
 */
export function Stage({ children }: { children: ReactNode }) {
  const { width, height } = useWindowDimensions();
  const scale = width > 0 && height > 0 ? Math.min(width / STAGE.width, height / STAGE.height) : 1;
  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, overflow: "hidden" }}>
      <View style={{ width: STAGE.width, height: STAGE.height, transform: [{ scale }], transformOrigin: "top left" }}>{children}</View>
    </View>
  );
}
