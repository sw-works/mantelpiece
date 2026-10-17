// Default for tooling and non-Vega builds: no music player, so reels play silently on the
// wall clock. Metro on Vega picks `musicPlayer.kepler.ts` first (see metro.config.js sourceExts).
import type { MusicPlayer } from "@mantelpiece/tv-ui";

export function musicPlayer(): MusicPlayer | null {
  return null;
}
