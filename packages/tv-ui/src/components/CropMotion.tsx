import { useMemo } from "react";
import { Animated, Image, View } from "react-native";
import { type CropWindow, type ReelSlide } from "../api/client";

export interface Frame {
  w: number;
  h: number;
}

/** The TV picture: every screen is laid out on a 1920x1080 stage. */
export const TV_FRAME: Frame = { w: 1920, h: 1080 };

/** Where a picture is drawn on the frame, in frame pixels. */
export interface CropRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

const clamp01 = (p: number) => (Number.isFinite(p) ? Math.min(1, Math.max(0, p)) : 0);
const lerp = (a: number, b: number, p: number) => a + (b - a) * p;

/** The crop window at progress `p` (0 to 1) of a slide's move. */
export function lerpWindow(from: CropWindow, to: CropWindow, p: number): CropWindow {
  const t = clamp01(p);
  return { cx: lerp(from.cx, to.cx, t), cy: lerp(from.cy, to.cy, t), z: lerp(from.z, to.z, t) };
}

/** The picture's size when it just covers the frame (`z = 1`). Only its aspect matters. */
export function coverSize(aspect: number, frame: Frame = TV_FRAME): { width: number; height: number } {
  const a = Number.isFinite(aspect) && aspect > 0 ? aspect : frame.w / frame.h;
  // A picture `a` wide and 1 high: s0 = max(F.w / w, F.h / h).
  const s0 = Math.max(frame.w / a, frame.h);
  return { width: a * s0, height: s0 };
}

/**
 * The contract's crop-window formula (contracts/api.md): the picture drawn at
 * `s = max(F.w / w, F.h / h) * z`, its window centre on the frame centre, then clamped so
 * the picture always covers the frame. A picture smaller than the frame on an axis (only
 * possible with `z < 1`, which the planner does not emit) is centred on that axis.
 */
export function cropRect(window: CropWindow, aspect: number, frame: Frame = TV_FRAME): CropRect {
  const base = coverSize(aspect, frame);
  const z = Number.isFinite(window.z) && window.z > 0 ? window.z : 1;
  const width = base.width * z;
  const height = base.height * z;
  const place = (centre: number, size: number, span: number) => {
    if (size <= span) return (span - size) / 2;
    const at = span / 2 - centre * size;
    return Math.min(0, Math.max(span - size, at));
  };
  return { left: place(window.cx, width, frame.w), top: place(window.cy, height, frame.h), width, height };
}

/**
 * The transform that draws a cover-sized picture (laid out at `coverSize`, top left on the
 * frame) at `rect`. React Native scales about the view's centre, so the translation makes
 * up for the growth on each side.
 */
export function rectTransform(rect: CropRect, base: { width: number; height: number }): { translateX: number; translateY: number; scale: number } {
  const scale = rect.width / base.width;
  return { translateX: rect.left + (base.width * (scale - 1)) / 2, translateY: rect.top + (base.height * (scale - 1)) / 2, scale };
}

/** Samples per slide for the piecewise-linear transform; the move is near-linear, so a few suffice. */
const SEGMENTS = 8;

/**
 * F-713: one reel slide, moving from its `from` crop window to its `to` window. Progress is
 * read from `clock` (milliseconds of the reel, driven by the music), never from an animation
 * of its own, so the picture moves in step with the music and holds when it pauses.
 */
export function CropMotion({
  uri,
  slide,
  clock,
  frame = TV_FRAME,
  testID,
}: {
  /** Resolved picture URL; nothing is drawn until it is known. */
  uri: string | null;
  slide: Pick<ReelSlide, "media" | "aspect" | "start_ms" | "end_ms" | "from" | "to">;
  /** The reel clock in milliseconds. */
  clock: Animated.Value;
  frame?: Frame;
  testID?: string;
}) {
  const base = useMemo(() => coverSize(slide.aspect, frame), [slide.aspect, frame]);
  const motion = useMemo(() => {
    const span = Math.max(1, slide.end_ms - slide.start_ms);
    const inputRange: number[] = [];
    const translateX: number[] = [];
    const translateY: number[] = [];
    const scale: number[] = [];
    for (let k = 0; k <= SEGMENTS; k++) {
      const p = k / SEGMENTS;
      const t = rectTransform(cropRect(lerpWindow(slide.from, slide.to, p), slide.aspect, frame), base);
      inputRange.push(slide.start_ms + p * span);
      translateX.push(t.translateX);
      translateY.push(t.translateY);
      scale.push(t.scale);
    }
    const at = (outputRange: number[]) => clock.interpolate({ inputRange, outputRange, extrapolate: "clamp" });
    return [{ translateX: at(translateX) }, { translateY: at(translateY) }, { scale: at(scale) }];
  }, [clock, slide.start_ms, slide.end_ms, slide.from, slide.to, slide.aspect, frame, base]);
  return (
    <View pointerEvents="none" style={{ position: "absolute", left: 0, top: 0, width: frame.w, height: frame.h, overflow: "hidden", backgroundColor: "#000" }}>
      {uri && (
        <Animated.View testID={testID} style={{ position: "absolute", left: 0, top: 0, width: base.width, height: base.height, transform: motion }}>
          <Image testID={`reel-image-${slide.media}`} source={{ uri }} style={{ width: base.width, height: base.height }} resizeMode="cover" />
        </Animated.View>
      )}
    </View>
  );
}
