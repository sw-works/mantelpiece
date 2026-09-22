// Default for tooling and non-Vega builds: no live player, so the Live screen says the TV
// cannot play live video. Metro on Vega picks `livePlayer.kepler.tsx` first.
import type { LivePlayer } from "@mantelpiece/tv-ui";

export function livePlayer(): LivePlayer | undefined {
  return undefined;
}
