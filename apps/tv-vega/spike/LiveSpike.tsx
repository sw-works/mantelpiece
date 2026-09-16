// Live-latency spike (specs/video-call-feasibility-2026-09-15.md, section 4). Plays a live
// HLS URL through hls.js on the Vega W3C media port and overlays the device's UTC clock so a
// screenshot shows the burned-in encoder clock next to the device clock: the difference is
// glass-to-glass lag. Select on the remote cycles the hls.js live-sync config and reloads.
//
// Lives outside src/ so the normal build, typecheck and CI never see it; ./vendor is not
// committed (Amazon-licensed polyfills, patched hls.js build). README.md here says how to run it.
/* eslint-disable @typescript-eslint/no-explicit-any -- hls.js and the polyfills are untyped vendor code */
import React, { useCallback, useEffect, useRef, useState } from "react";
import { Pressable, StyleSheet, Text, View } from "react-native";
import { KeplerVideoSurfaceView, VideoPlayer } from "@amazon-devices/react-native-w3cmedia";
import * as HlsLib from "./vendor/dist/hls.mjs";
import Document from "./vendor/polyfills/DocumentPolyfill";
import Element from "./vendor/polyfills/ElementPolyfill";
import TextDecoderPolyfill from "./vendor/polyfills/TextDecoderPolyfill";
import W3CMediaPolyfill from "./vendor/polyfills/W3CMediaPolyfill";
import MiscPolyfill from "./vendor/polyfills/MiscPolyfill";

interface Mode {
  name: string;
  config: Record<string, unknown>;
}

// hls.js live-sync settings, from stock to aggressive. IVS low-latency channels serve 2 s
// segments with EXT-X-PROGRAM-DATE-TIME, so hls.js can report its own latency estimate.
// IVS advertises EXT-X-TARGETDURATION 6 over 2 s segments, so the count-based default aims
// 18 s behind the edge; the seconds-based settings below aim at 2 to 3 segments.
// Measured on the Virtual Device 2026-09-15 (specs/video-call-feasibility-2026-09-15.md):
// 4 s target plays at 9.3 s glass-to-glass, 2.5 s never starts, stock lands 30 s back.
const MODES: Mode[] = [
  { name: "liveSyncDuration 3s", config: { liveSyncDuration: 3, liveMaxLatencyDuration: 8, maxBufferLength: 6 } },
  { name: "liveSyncDuration 4s + catch-up 1.5x", config: { liveSyncDuration: 4, liveMaxLatencyDuration: 8, maxBufferLength: 6, maxLiveSyncPlaybackRate: 1.5 } },
  { name: "liveSyncDuration 4s", config: { liveSyncDuration: 4, liveMaxLatencyDuration: 10, maxBufferLength: 6 } },
  { name: "stock: liveSyncDurationCount 3", config: { liveSyncDurationCount: 3 } },
];

function modeAt(i: number): Mode {
  return MODES[i % MODES.length] as Mode;
}

function utcClock(ms: number): string {
  return new Date(ms).toISOString().slice(11, 23);
}

let polyfilled = false;
function installPolyfills() {
  if (polyfilled) return;
  polyfilled = true;
  // hls.js clears timers it never set (handle -1 / undefined); Hermes on Vega throws
  // "clearTimeout called with an invalid handle" and the live reload loop dies.
  const g = globalThis as any;
  for (const name of ["clearTimeout", "clearInterval"]) {
    const orig = g[name];
    g[name] = (h: unknown) => {
      if (h === undefined || h === null || h === -1) return;
      try { orig(h); } catch (e) { console.log(`spike: ${name} ignored`, String(e)); }
    };
    if (g.self && g.self !== g) g.self[name] = g[name];
    if (g.window && g.window !== g) g.window[name] = g[name];
  }
  Document.install();
  Element.install();
  TextDecoderPolyfill.install();
  W3CMediaPolyfill.install();
  MiscPolyfill.install();
}

export function LiveSpike({ uri }: { uri: string }) {
  const video = useRef<VideoPlayer | null>(null);
  const hls = useRef<any>(null);
  const surface = useRef<string | null>(null);
  const [modeIdx, setModeIdx] = useState(0);
  const [clock, setClock] = useState(utcClock(Date.now()));
  const [stats, setStats] = useState("starting");
  const [status, setStatus] = useState("init");

  const startPlayer = useCallback(async (mode: Mode) => {
    installPolyfills();
    if (hls.current) {
      try { hls.current.destroy(); } catch (e) { console.log("spike: destroy", e); }
      hls.current = null;
    }
    if (!video.current) {
      const vp = new VideoPlayer();
      (globalThis as any).gmedia = vp;
      await vp.initialize();
      vp.addEventListener("error", () => setStatus("video error"));
      vp.addEventListener("playing", () => setStatus("playing"));
      vp.addEventListener("waiting", () => setStatus("waiting"));
      vp.autoplay = true;
      video.current = vp;
      if (surface.current) vp.setSurfaceHandle(surface.current);
    }
    const Hls = (HlsLib as any).Hls;
    const player = new Hls({
      enableWorker: false,
      lowLatencyMode: true,
      debug: false,
      backBufferLength: 10,
      maxBufferLength: 10,
      maxMaxBufferLength: 20,
      ...mode.config,
    });
    player.on((HlsLib as any).Events.ERROR, (_e: unknown, data: any) => {
      console.log("spike: hls error", data?.type, data?.details, data?.fatal);
      setStatus(`hls ${data?.details}${data?.fatal ? " FATAL" : ""}`);
    });
    const Ev = (HlsLib as any).Events;
    player.on(Ev.MANIFEST_PARSED, () => setStatus("manifest parsed"));
    player.on(Ev.LEVEL_LOADED, (_e: unknown, d: any) => {
      const det = d?.details;
      console.log(`spike: level loaded target ${det?.targetduration}s frags ${det?.fragments?.length} parts ${det?.partList?.length ?? 0} live ${det?.live} edge ${fmt(det?.edge)}`);
      setStatus(`level ${det?.targetduration}s x${det?.fragments?.length}`);
    });
    player.on(Ev.FRAG_LOADED, (_e: unknown, d: any) => console.log(`spike: frag loaded sn ${d?.frag?.sn} ${fmt(d?.frag?.duration)}s`));
    player.on(Ev.FRAG_BUFFERED, (_e: unknown, d: any) => { console.log(`spike: frag buffered sn ${d?.frag?.sn}`); setStatus(`buffered sn ${d?.frag?.sn}`); });
    player.on(Ev.BUFFER_CODECS, (_e: unknown, d: any) => console.log(`spike: buffer codecs ${JSON.stringify(Object.keys(d ?? {}))}`));
    hls.current = player;
    player.loadSource(uri);
    player.attachMedia(video.current);
    video.current?.play();
    console.log(`spike: loaded mode "${mode.name}" ${JSON.stringify(mode.config)}`);
  }, [uri]);

  useEffect(() => {
    void startPlayer(modeAt(modeIdx));
  }, [modeIdx, startPlayer]);

  useEffect(() => {
    const t = setInterval(() => setClock(utcClock(Date.now())), 50);
    const s = setInterval(() => {
      const h = hls.current;
      const v = video.current;
      if (!h || !v) return;
      const ct = v.currentTime;
      const buffered = v.buffered;
      const end = buffered && buffered.length ? buffered.end(buffered.length - 1) : NaN;
      const line = `hls.latency ${fmt(h.latency)}s target ${fmt(h.targetLatency)}s drift ${fmt(h.drift)} ct ${fmt(ct)} ahead ${fmt(end - ct)}s`;
      setStats(line);
      console.log(`spike: ${utcClock(Date.now())} ${line}`);
    }, 1000);
    return () => { clearInterval(t); clearInterval(s); };
  }, []);

  const onSurfaceViewCreated = (handle: string) => {
    surface.current = handle;
    video.current?.setSurfaceHandle(handle);
    video.current?.play();
  };
  const onSurfaceViewDestroyed = (handle: string) => {
    video.current?.clearSurfaceHandle(handle);
    surface.current = null;
  };

  return (
    <View style={styles.root}>
      <KeplerVideoSurfaceView style={styles.surface} onSurfaceViewCreated={onSurfaceViewCreated} onSurfaceViewDestroyed={onSurfaceViewDestroyed} />
      <View style={styles.overlay} pointerEvents="box-none">
        <Text style={styles.clock}>device {clock}</Text>
        <Text style={styles.line}>{status} | {modeAt(modeIdx).name}</Text>
        <Text style={styles.line}>{stats}</Text>
        <Pressable hasTVPreferredFocus onPress={() => setModeIdx((i) => (i + 1) % MODES.length)} style={(state) => [styles.button, (state as { focused?: boolean }).focused && styles.focused]}>
          <Text style={styles.line}>Select: next hls.js mode</Text>
        </Pressable>
      </View>
    </View>
  );
}

function fmt(n: unknown): string {
  return typeof n === "number" && Number.isFinite(n) ? n.toFixed(2) : "-";
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: "black" },
  surface: { position: "absolute", top: 0, left: 0, right: 0, bottom: 0, zIndex: 0 },
  overlay: { position: "absolute", left: 24, top: 24, zIndex: 2 },
  clock: { color: "#fff", fontSize: 56, fontFamily: "monospace", backgroundColor: "rgba(0,0,0,0.7)", paddingHorizontal: 12 },
  line: { color: "#fff", fontSize: 22, backgroundColor: "rgba(0,0,0,0.7)", paddingHorizontal: 12, marginTop: 4 },
  button: { marginTop: 8, borderWidth: 2, borderColor: "transparent" },
  focused: { borderColor: "#0f6" },
});
