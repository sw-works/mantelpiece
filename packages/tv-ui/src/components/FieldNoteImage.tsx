import { useState } from "react";
import { Pressable, Text, View } from "react-native";
import { type FieldNoteSummary, type NormBox, type TvApi } from "../api/client";
import { theme } from "../theme";
import { RemoteImage } from "./RemoteImage";

/** Where a `contain` image of `aspect` (width / height) sits inside a `w` × `h` box. */
export function containedRect(aspect: number, w: number, h: number): NormBox {
  if (aspect >= w / h) {
    const height = w / aspect;
    return { left: 0, top: (h - height) / 2, width: w, height };
  }
  const width = h * aspect;
  return { left: (w - width) / 2, top: 0, width, height: h };
}

/** The map's box on screen: the note's placement mapped through the contained photo. */
export function mapRect(note: FieldNoteSummary, aspect: number, w: number, h: number): NormBox {
  const r = containedRect(aspect, w, h);
  return {
    left: r.left + note.placement.left * r.width,
    top: r.top + note.placement.top * r.height,
    width: note.placement.width * r.width,
    height: note.placement.height * r.height,
  };
}

/**
 * F-711: a photo shown whole with its field note laid over it. The layer is a separate
 * image the size of the photo, so two `contain` images in one box line up exactly; the
 * photo itself is never altered. With `onOpen`, the map is a remote focus target: focused,
 * it comes up to full strength with a soft outline and a hint, and Select opens it.
 */
export function FieldNoteImage({
  api,
  token,
  mediaId,
  note,
  width,
  height,
  onOpen,
  onFocusChange,
  preferred = false,
}: {
  api: TvApi;
  token: string;
  mediaId: string;
  note: FieldNoteSummary;
  width: number;
  height: number;
  onOpen?: () => void;
  onFocusChange?: (focused: boolean) => void;
  /** Take focus on mount, e.g. when coming back from "This place". */
  preferred?: boolean;
}) {
  const [aspect, setAspect] = useState<number | null>(null);
  const [focused, setFocused] = useState(false);
  const rect = aspect ? mapRect(note, aspect, width, height) : null;
  return (
    <View testID="field-note" style={{ width, height }}>
      <RemoteImage api={api} token={token} mediaId={mediaId} rendition="tv" width={width} height={height} radius={0} resizeMode="contain" onSize={(w, h) => h > 0 && setAspect(w / h)} />
      <View pointerEvents="none" style={{ position: "absolute", left: 0, top: 0, opacity: focused ? 1 : 0.92 }}>
        <RemoteImage api={api} token={token} mediaId={mediaId} rendition="fieldnote" width={width} height={height} radius={0} resizeMode="contain" />
      </View>
      {rect && onOpen && (
        <Pressable
          testID="field-note-map"
          accessibilityRole="button"
          accessibilityLabel="Open the map"
          hasTVPreferredFocus={preferred}
          onFocus={() => {
            setFocused(true);
            onFocusChange?.(true);
          }}
          onBlur={() => {
            setFocused(false);
            onFocusChange?.(false);
          }}
          onPress={onOpen}
          style={{
            position: "absolute",
            left: rect.left - 8,
            top: rect.top - 8,
            width: rect.width + 16,
            height: rect.height + 16,
            borderRadius: 22,
            borderWidth: 3,
            borderColor: focused ? "#ffffffb3" : "transparent",
            backgroundColor: focused ? "#ffffff0d" : "transparent",
          }}
        >
          {focused && (
            <View style={{ position: "absolute", left: 0, right: 0, bottom: -52, alignItems: "center" }}>
              <Text testID="field-note-hint" style={{ color: theme.fg, fontSize: theme.small, backgroundColor: "#0b0b0fcc", paddingHorizontal: 16, paddingVertical: 6, borderRadius: 16 }}>
                Select · open the map
              </Text>
            </View>
          )}
        </Pressable>
      )}
    </View>
  );
}
