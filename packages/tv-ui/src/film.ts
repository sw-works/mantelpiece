// A day film (F-714) is a progressive MP4. The picture comes from the platform shell: on
// Vega the system W3C media player in URL mode. The shared UI only knows this shape.

import { type ComponentType } from "react";

export type FilmPlayerState = "starting" | "playing" | "ended" | "error";

export interface FilmPlayerProps {
  /** Presigned URL of the MP4. */
  uri: string;
  onState?: (state: FilmPlayerState) => void;
}

export type FilmPlayer = ComponentType<FilmPlayerProps>;
