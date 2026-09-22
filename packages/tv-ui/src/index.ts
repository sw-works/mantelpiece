// Public surface of the shared TV UI. Platform shells (apps/tv-vega, later apps/tv-fireos)
// register `App` and inject platform-specific services such as a persistent SessionStore.
export { App } from "./App";
export type { AppProps, Route } from "./App";
export { TvApi, clockHints, creditFor, slides, storyOf } from "./api/client";
export type { AudioPlayer } from "./audio";
export type { LivePlayer, LivePlayerProps, LivePlayerState } from "./live";
export { Ambient } from "./screens/Ambient";
export { Ask } from "./screens/Ask";
export { JoinTrip } from "./screens/JoinTrip";
export { MapView } from "./screens/MapView";
export { People } from "./screens/People";
export { ReviewChanges } from "./screens/ReviewChanges";
export { Slideshow } from "./screens/Slideshow";
export type { NowShowing } from "./screens/Slideshow";
export { KeyValueSessionStore, MemorySessionStore, SESSION_KEY, newDeviceId } from "./session";
export type { AsyncStorageLike, SessionStore } from "./session";
export { theme } from "./theme";
