/**
 * The reel's music and its clock (F-713). The shared UI never bundles a media module; the
 * platform shell hands in a player when it has one (Vega: the system W3C media port). The
 * reel reads `positionMs()` to place its cuts on the music's bar lines and falls back to a
 * silent, wall-clock reel when there is no player or the player has no position.
 */
export interface MusicPlayer {
  /** Load a track and be ready to play it from 0. Rejects when it cannot. */
  load(url: string): Promise<void>;
  play(): void;
  pause(): void;
  /** Playback position in milliseconds, or null when it is not a usable clock (not loaded, failed, ended). */
  positionMs(): number | null;
  /** Stop and release the track. */
  stop(): void;
}
