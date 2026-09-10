// Public surface of the shared TV UI. Platform shells (apps/tv-vega, later apps/tv-fireos)
// register `App` and inject platform-specific services such as a persistent SessionStore.
export { App } from "./App";
export type { AppProps, Route } from "./App";
export { TvApi } from "./api/client";
export { MemorySessionStore, newDeviceId } from "./session";
export type { SessionStore } from "./session";
export { theme } from "./theme";
