// Day films on Vega (F-714): a progressive MP4 (H.264 + AAC) played by the system-deployed
// W3C media port's VideoPlayer in URL mode (`src = uri`, no hls.js), drawn on a
// KeplerVideoSurfaceView. Same surface handling and clean-up as livePlayer.kepler.tsx. The
// source is set only once the surface is attached: the Vega forums report
// MEDIA_ERR_SRC_NOT_SUPPORTED when `src` is set before `setSurfaceHandle`.
//
// UNVERIFIED ON A DEVICE: written against the typings of @amazon-devices/react-native-w3cmedia
// 2.3.4-rn-83 and Amazon's URL-mode guidance; not yet run on the Vega Virtual Device or a
// Fire TV Stick. Not handled yet: releasing the player when the app goes to the background.
/* eslint-disable @typescript-eslint/no-require-imports -- the system media port is required at runtime, see livePlayer.kepler.tsx */
import { type ComponentType, useEffect, useRef } from "react";
import { StyleSheet, View, type ViewStyle } from "react-native";
import type { FilmPlayer, FilmPlayerProps } from "@mantelpiece/tv-ui";

// The few members used here; an `import` makes tsc type-check react-native-kepler's sources.
interface VideoPlayer {
  autoplay: boolean;
  src: string;
  initialize(): Promise<void>;
  deinitialize(): Promise<void>;
  play(): Promise<void> | void;
  setSurfaceHandle(handle: string): void;
  clearSurfaceHandle(handle: string): void;
  addEventListener(type: string, listener: () => void): void;
}

interface W3cMediaModule {
  VideoPlayer: new () => VideoPlayer;
  KeplerVideoSurfaceView: ComponentType<{ style?: ViewStyle; onSurfaceViewCreated: (handle: string) => void; onSurfaceViewDestroyed: (handle: string) => void }>;
}

function w3cMedia(): W3cMediaModule | null {
  try {
    const m = require("@amazon-devices/react-native-w3cmedia") as Partial<W3cMediaModule>;
    return typeof m?.VideoPlayer === "function" && m.KeplerVideoSurfaceView ? (m as W3cMediaModule) : null;
  } catch {
    return null;
  }
}

const W3cMedia = w3cMedia();

function VegaFilmPlayer({ uri, onState }: FilmPlayerProps) {
  const media = W3cMedia!;
  const video = useRef<VideoPlayer | null>(null);
  const surface = useRef<string | null>(null);
  const started = useRef(false);
  const report = useRef(onState);
  report.current = onState;

  // Both the initialised player and the surface are needed; whichever arrives last starts it.
  const start = useRef<() => void>(() => undefined);
  start.current = () => {
    const vp = video.current;
    const handle = surface.current;
    if (!vp || !handle || started.current) return;
    started.current = true;
    vp.setSurfaceHandle(handle);
    vp.src = uri;
    try {
      void Promise.resolve(vp.play()).catch(() => undefined);
    } catch {
      // autoplay starts it
    }
  };

  useEffect(() => {
    let alive = true;
    started.current = false;
    report.current?.("starting");
    (async () => {
      const vp = new media.VideoPlayer();
      await vp.initialize();
      if (!alive) {
        void vp.deinitialize().catch(() => undefined);
        return;
      }
      vp.autoplay = true;
      vp.addEventListener("playing", () => alive && report.current?.("playing"));
      vp.addEventListener("ended", () => alive && report.current?.("ended"));
      vp.addEventListener("error", () => alive && report.current?.("error"));
      video.current = vp;
      start.current();
    })().catch(() => alive && report.current?.("error"));
    return () => {
      alive = false;
      const vp = video.current;
      video.current = null;
      if (vp) {
        if (surface.current) vp.clearSurfaceHandle(surface.current);
        void vp.deinitialize().catch(() => undefined);
      }
    };
  }, [uri, media]);

  const { KeplerVideoSurfaceView } = media;
  return (
    <View style={StyleSheet.absoluteFill}>
      <KeplerVideoSurfaceView
        style={StyleSheet.absoluteFill}
        onSurfaceViewCreated={(handle: string) => {
          surface.current = handle;
          if (started.current) video.current?.setSurfaceHandle(handle);
          else start.current();
        }}
        onSurfaceViewDestroyed={(handle: string) => {
          video.current?.clearSurfaceHandle(handle);
          surface.current = null;
        }}
      />
    </View>
  );
}

export function filmPlayer(): FilmPlayer | undefined {
  return W3cMedia ? VegaFilmPlayer : undefined;
}
