// Default for tooling and non-Vega builds: no film player, so the Film screen says the TV
// cannot play films yet. Metro on Vega picks `filmPlayer.kepler.tsx` first.
import type { FilmPlayer } from "@mantelpiece/tv-ui";

export function filmPlayer(): FilmPlayer | undefined {
  return undefined;
}
