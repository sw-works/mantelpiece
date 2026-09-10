// Persistent TV session on Vega (research R2). The Vega React Native (react-native-kepler 4)
// keeps AsyncStorage exported from "react-native" as a system module, so no package is
// bundled; upstream React Native has no such export, hence the runtime lookup. Falls back
// to memory when the export is missing, which is what happens under jest or on a platform
// without it. Not yet verified on a Vega device.

import { KeyValueSessionStore, MemorySessionStore, type AsyncStorageLike, type SessionStore } from "@memorylane/tv-ui";
import * as ReactNative from "react-native";

export function nativeAsyncStorage(): AsyncStorageLike | null {
  const candidate = (ReactNative as unknown as { AsyncStorage?: Partial<AsyncStorageLike> }).AsyncStorage;
  if (candidate && typeof candidate.getItem === "function" && typeof candidate.setItem === "function" && typeof candidate.removeItem === "function") {
    return candidate as AsyncStorageLike;
  }
  return null;
}

export function sessionStore(): SessionStore {
  const kv = nativeAsyncStorage();
  return kv ? new KeyValueSessionStore(kv) : new MemorySessionStore();
}
