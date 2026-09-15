// Persistent TV session on Vega (research R2). The system-deployed port of the community
// async-storage package (@amazon-devices/react-native-async-storage__async-storage) writes
// under the app's /data sandbox; the legacy AsyncStorage export on react-native-kepler is
// deprecated and, on the Virtual Device, failed with "Application data root path is not
// set" (verified 2026-09-14). Falls back to memory when the module is missing, which is
// what happens under jest or on a platform without it.

import { KeyValueSessionStore, MemorySessionStore, type AsyncStorageLike, type SessionStore } from "@memorylane/tv-ui";

export function nativeAsyncStorage(): AsyncStorageLike | null {
  let candidate: Partial<AsyncStorageLike> | undefined;
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const mod = require("@amazon-devices/react-native-async-storage__async-storage") as { default?: Partial<AsyncStorageLike> } & Partial<AsyncStorageLike>;
    candidate = mod.default ?? mod;
  } catch {
    return null;
  }
  if (candidate && typeof candidate.getItem === "function" && typeof candidate.setItem === "function" && typeof candidate.removeItem === "function") {
    return candidate as AsyncStorageLike;
  }
  return null;
}

export function sessionStore(): SessionStore {
  const kv = nativeAsyncStorage();
  return kv ? new KeyValueSessionStore(kv) : new MemorySessionStore();
}
