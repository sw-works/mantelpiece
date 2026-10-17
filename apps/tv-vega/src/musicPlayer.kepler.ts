// Reel music on Vega (F-713): the system-deployed W3C media port's AudioPlayer in URL mode,
// whose position is the reel's clock. The port is reached by a runtime `require`, never
// bundled (CLAUDE.md, constitution II); without it, reels play silently on the wall clock.
//
// UNVERIFIED ON A DEVICE: written against the typings of @amazon-devices/react-native-w3cmedia
// 2.3.4-rn-83 (AudioPlayer: initialize, src, play, pause, currentTime in seconds,
// deinitialize; standard media events) and the port's README, which says URL mode plays
// flat files such as MP3 and MP4. Not yet run on the Vega Virtual Device or a Fire TV Stick:
// whether an .m4a (AAC in MP4) plays in URL mode, how fine-grained `currentTime` is, and
// whether `canplay` fires for a presigned S3 URL all need checking there.
/* eslint-disable @typescript-eslint/no-require-imports -- the system media port is required at runtime, see above */
import type { MusicPlayer } from "@mantelpiece/tv-ui";

// The few members used here. An `import` of the port makes tsc type-check
// react-native-kepler's own sources, which do not pass strict mode (see livePlayer.kepler.tsx).
interface W3cAudioPlayer {
  src: string;
  autoplay: boolean;
  readonly currentTime: number;
  initialize(): Promise<void>;
  deinitialize(): Promise<void>;
  play(): Promise<void> | void;
  pause(): void;
  addEventListener(type: string, listener: () => void): void;
}

interface W3cMediaModule {
  AudioPlayer?: new (contentType?: number, usage?: number) => W3cAudioPlayer;
  AudioContentType?: { CONTENT_TYPE_MUSIC?: number };
  AudioUsageType?: { USAGE_MEDIA?: number };
}

/** The music waits this long for the track to be playable before the reel goes silent. */
export const LOAD_TIMEOUT_MS = 10_000;

function w3cMedia(): W3cMediaModule | null {
  try {
    const m = require("@amazon-devices/react-native-w3cmedia") as W3cMediaModule;
    return typeof m?.AudioPlayer === "function" ? m : null;
  } catch {
    return null;
  }
}

export function musicPlayer(): MusicPlayer | null {
  const media = w3cMedia();
  const Ctor = media?.AudioPlayer;
  if (!media || !Ctor) return null;
  let player: W3cAudioPlayer | null = null;
  // False once the track failed or ran out: its position is no longer a clock.
  let usable = false;
  // Bumped by every load and stop, so a slow load that was overtaken lets its player go.
  let generation = 0;

  const release = () => {
    const p = player;
    player = null;
    usable = false;
    if (!p) return;
    try {
      p.pause();
    } catch {
      // never started
    }
    void p.deinitialize().catch(() => undefined);
  };

  return {
    async load(url: string) {
      release();
      const mine = ++generation;
      const p = new Ctor(media.AudioContentType?.CONTENT_TYPE_MUSIC ?? 2, media.AudioUsageType?.USAGE_MEDIA ?? 1);
      await p.initialize();
      if (mine !== generation) {
        void p.deinitialize().catch(() => undefined);
        throw new Error("superseded");
      }
      player = p;
      p.autoplay = false;
      try {
        await whenPlayable(p, url);
      } catch (e) {
        if (mine === generation) release();
        throw e;
      }
      if (mine !== generation) throw new Error("superseded");
      usable = true;
    },
    play() {
      const p = player;
      if (!p) return;
      try {
        void Promise.resolve(p.play()).catch(() => {
          usable = false;
        });
      } catch {
        usable = false;
      }
    },
    pause() {
      try {
        player?.pause();
      } catch {
        // nothing playing
      }
    },
    positionMs() {
      if (!player || !usable) return null;
      const seconds = player.currentTime;
      return Number.isFinite(seconds) ? seconds * 1000 : null;
    },
    stop() {
      generation++;
      release();
    },
  };

  /** Sets the source and waits until the track can play; a failure later makes the position unusable. */
  function whenPlayable(p: W3cAudioPlayer, url: string): Promise<void> {
    const unusable = () => {
      if (player === p) usable = false;
    };
    return new Promise<void>((resolve, reject) => {
      let done = false;
      const finish = (ok: boolean) => {
        if (done) return;
        done = true;
        clearTimeout(timer);
        if (ok) resolve();
        else reject(new Error("music did not load"));
      };
      const timer = setTimeout(() => finish(false), LOAD_TIMEOUT_MS);
      p.addEventListener("canplay", () => finish(true));
      p.addEventListener("loadeddata", () => finish(true));
      p.addEventListener("error", () => {
        unusable();
        finish(false);
      });
      // A track shorter than the reel: the reel carries on on the wall clock.
      p.addEventListener("ended", unusable);
      p.src = url;
    });
  }
}
