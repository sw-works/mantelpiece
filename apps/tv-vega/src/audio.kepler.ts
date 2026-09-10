// Narration audio on Vega (F-709, research R10). The reference samples pull in
// `@amazon-devices/react-native-w3cmedia` as a package, which the TV app rule forbids
// bundling. The Vega React Native (react-native-kepler 4) may expose the W3C media port as
// a system module the way it exposes AsyncStorage, so this looks for an `Audio`
// constructor at runtime and never imports a package. When nothing is found, playback is
// off and the Slideshow shows the story as captions. Not yet verified on a Vega device or
// the Virtual Device: the SDK is not installed on this machine.

import type { AudioPlayer } from "@memorylane/tv-ui";
import * as ReactNative from "react-native";

interface W3cAudio {
  src: string;
  play(): Promise<void> | void;
  pause(): void;
  addEventListener?(type: string, listener: () => void): void;
}

type AudioCtor = new (src?: string) => W3cAudio;

/** The system-deployed `Audio` constructor, or null when this build has none. */
export function nativeAudio(): AudioCtor | null {
  const fromRn = (ReactNative as unknown as { Audio?: unknown }).Audio;
  const fromGlobal = (globalThis as unknown as { Audio?: unknown }).Audio;
  const candidate = fromRn ?? fromGlobal;
  return typeof candidate === "function" ? (candidate as AudioCtor) : null;
}

export function audioPlayer(): AudioPlayer | null {
  const Ctor = nativeAudio();
  if (!Ctor) return null;
  let current: W3cAudio | null = null;
  return {
    async play(url: string) {
      current?.pause();
      const audio = new Ctor(url);
      current = audio;
      await audio.play();
    },
    stop() {
      current?.pause();
      current = null;
    },
  };
}
