// Default for tooling and non-Vega builds: no platform audio, so the Slideshow shows
// captions. Metro on Vega picks `audio.kepler.ts` first (see metro.config.js sourceExts).
import type { AudioPlayer } from "@mantelpiece/tv-ui";

export function audioPlayer(): AudioPlayer | null {
  return null;
}
