/**
 * Narration playback (F-709). The shared UI never bundles a media module; the platform
 * shell hands in a player when it has one (Vega: the system media port, if present) and
 * the Slideshow falls back to captions when it does not.
 */
export interface AudioPlayer {
  play(url: string): Promise<void>;
  stop(): void;
}
