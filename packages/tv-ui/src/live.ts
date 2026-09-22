// The live picture comes from the platform shell: on Vega a W3C media player driven by
// hls.js, on Fire OS whatever plays HLS there. The shared UI only knows this shape.

import { type ComponentType } from "react";

export type LivePlayerState = "starting" | "playing" | "stalled" | "error";

export interface LivePlayerProps {
  /** HLS playback URL of the broadcast. */
  uri: string;
  onState?: (state: LivePlayerState) => void;
}

export type LivePlayer = ComponentType<LivePlayerProps>;
