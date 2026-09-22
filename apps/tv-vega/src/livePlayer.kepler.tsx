// Live video on Vega: hls.js (Amazon's Vega patch set, scripts/vega-hls.sh) feeding the
// system-deployed W3C media player in MSE mode, drawn on a KeplerVideoSurfaceView. The
// settings are the spike's best (specs/video-call-feasibility-2026-09-15.md section 7):
// IVS serves 2 s segments but advertises a 6 s target duration, so a seconds-based
// liveSyncDuration of 3 keeps the picture 7.9 s behind the camera instead of 30 s.
/* eslint-disable @typescript-eslint/no-explicit-any, @typescript-eslint/no-require-imports -- hls.js is untyped vendor code; see the require note below */
import { type ComponentType, useEffect, useRef } from "react";
import { StyleSheet, View, type ViewStyle } from "react-native";
import type { LivePlayer, LivePlayerProps } from "@memorylane/tv-ui";
import * as VegaHls from "vega-hls";

// The W3C media port by `require` with the few members used here: an `import` makes tsc
// type-check react-native-kepler's own TypeScript sources, which do not pass strict mode.
interface VideoPlayer {
  autoplay: boolean;
  initialize(): Promise<void>;
  deinitialize(): Promise<void>;
  play(): void;
  setSurfaceHandle(handle: string): void;
  clearSurfaceHandle(handle: string): void;
  addEventListener(type: string, listener: () => void): void;
}
const W3cMedia = require("@amazon-devices/react-native-w3cmedia") as {
  VideoPlayer: new () => VideoPlayer;
  KeplerVideoSurfaceView: ComponentType<{ style?: ViewStyle; onSurfaceViewCreated: (handle: string) => void; onSurfaceViewDestroyed: (handle: string) => void }>;
};
const { KeplerVideoSurfaceView } = W3cMedia;

export const HLS_CONFIG = {
  enableWorker: false,
  lowLatencyMode: true,
  liveSyncDuration: 3,
  liveMaxLatencyDuration: 8,
  maxBufferLength: 6,
  maxMaxBufferLength: 20,
  backBufferLength: 10,
};

function VegaLivePlayer({ uri, onState }: LivePlayerProps) {
  const video = useRef<VideoPlayer | null>(null);
  const hls = useRef<any>(null);
  const surface = useRef<string | null>(null);
  const report = useRef(onState);
  report.current = onState;

  useEffect(() => {
    let alive = true;
    VegaHls.installPolyfills();
    (async () => {
      const vp = new W3cMedia.VideoPlayer();
      await vp.initialize();
      if (!alive) {
        void vp.deinitialize();
        return;
      }
      vp.autoplay = true;
      vp.addEventListener("playing", () => report.current?.("playing"));
      vp.addEventListener("waiting", () => report.current?.("stalled"));
      vp.addEventListener("error", () => report.current?.("error"));
      video.current = vp;
      if (surface.current) vp.setSurfaceHandle(surface.current);
      const player = new VegaHls.Hls(HLS_CONFIG);
      player.on(VegaHls.Events.ERROR, (_e: unknown, data: any) => {
        if (data?.fatal) report.current?.("error");
      });
      hls.current = player;
      player.loadSource(uri);
      player.attachMedia(vp);
      vp.play();
    })().catch(() => report.current?.("error"));
    return () => {
      alive = false;
      try {
        hls.current?.destroy();
      } catch {
        // already torn down
      }
      hls.current = null;
      const vp = video.current;
      video.current = null;
      if (vp) {
        if (surface.current) vp.clearSurfaceHandle(surface.current);
        void vp.deinitialize().catch(() => undefined);
      }
    };
  }, [uri]);

  return (
    <View style={StyleSheet.absoluteFill}>
      <KeplerVideoSurfaceView
        style={StyleSheet.absoluteFill}
        onSurfaceViewCreated={(handle: string) => {
          surface.current = handle;
          video.current?.setSurfaceHandle(handle);
          video.current?.play();
        }}
        onSurfaceViewDestroyed={(handle: string) => {
          video.current?.clearSurfaceHandle(handle);
          surface.current = null;
        }}
      />
    </View>
  );
}

export function livePlayer(): LivePlayer | undefined {
  return VegaHls.available ? VegaLivePlayer : undefined;
}
