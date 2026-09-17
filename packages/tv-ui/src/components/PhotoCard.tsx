import { useState } from "react";
import { Pressable, Text, View } from "react-native";
import { type TvApi } from "../api/client";
import { RemoteImage } from "./RemoteImage";
import { theme } from "../theme";

/**
 * A photo as a D-pad target: 4px white ring and a lift when focused, never a colour
 * change on the picture. Optional title and meta over a dark band, optional badge.
 */
export function PhotoCard({
  api,
  token,
  mediaId,
  width,
  height,
  title,
  meta,
  badge,
  onPress,
  preferred = false,
  testID,
  caption,
}: {
  api: TvApi;
  token: string;
  mediaId: string | null | undefined;
  width: number;
  height: number;
  title?: string;
  meta?: string;
  badge?: string;
  onPress?: () => void;
  preferred?: boolean;
  testID?: string;
  /** Line under the card, outside the frame (credit, day). */
  caption?: string;
}) {
  const [focused, setFocused] = useState(false);
  const body = (
    <View
      style={{
        width,
        height,
        borderRadius: 14,
        overflow: "hidden",
        backgroundColor: theme.panel,
        borderWidth: 4,
        borderColor: focused ? theme.focus : "transparent",
        transform: [{ scale: focused ? 1.06 : 1 }],
      }}
    >
      {mediaId ? <RemoteImage api={api} token={token} mediaId={mediaId} rendition="thumb" width={width - 8} height={height - 8} radius={10} /> : null}
      {(title || meta) && (
        <View style={{ position: "absolute", left: 0, right: 0, bottom: 0, padding: 16, backgroundColor: "#000000aa", gap: 2 }}>
          {title ? (
            <Text numberOfLines={2} style={{ color: theme.fg, fontSize: 26, fontWeight: "600", lineHeight: 30 }}>
              {title}
            </Text>
          ) : null}
          {meta ? <Text style={{ color: "#d5d5dc", fontSize: theme.credit }}>{meta}</Text> : null}
        </View>
      )}
      {badge ? (
        <View style={{ position: "absolute", left: 12, top: 12, paddingHorizontal: 12, paddingVertical: 6, borderRadius: 10, backgroundColor: theme.accent }}>
          <Text style={{ color: theme.onAccent, fontSize: theme.credit, fontWeight: "700", letterSpacing: 1, textTransform: "uppercase" }}>{badge}</Text>
        </View>
      ) : null}
    </View>
  );
  if (!onPress) {
    return (
      <View style={{ gap: 8 }}>
        {body}
        {caption ? <Text style={{ color: theme.muted, fontSize: theme.credit }}>{caption}</Text> : null}
      </View>
    );
  }
  return (
    <View style={{ gap: 8 }}>
      <Pressable testID={testID} accessibilityRole="button" hasTVPreferredFocus={preferred} onFocus={() => setFocused(true)} onBlur={() => setFocused(false)} onPress={onPress}>
        {body}
      </Pressable>
      {caption ? <Text style={{ color: theme.muted, fontSize: theme.credit }}>{caption}</Text> : null}
    </View>
  );
}
