// Where the TV keeps its session. Persistent storage on Vega goes through the Kepler
// storage API, which is a native surface; until that spike passes the session lives in
// memory and the TV re-pairs after a cold start.

export interface SessionStore {
  load(): Promise<{ token: string; household: string; deviceId: string } | null>;
  save(session: { token: string; household: string; deviceId: string }): Promise<void>;
  clear(): Promise<void>;
}

export class MemorySessionStore implements SessionStore {
  private value: { token: string; household: string; deviceId: string } | null = null;
  async load() {
    return this.value;
  }
  async save(session: { token: string; household: string; deviceId: string }) {
    this.value = session;
  }
  async clear() {
    this.value = null;
  }
}

export function newDeviceId(random: () => number = Math.random): string {
  const alphabet = "abcdefghijklmnopqrstuvwxyz0123456789";
  let out = "tv-";
  for (let i = 0; i < 16; i++) out += alphabet[Math.floor(random() * alphabet.length)];
  return out;
}
