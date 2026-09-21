import { describe, expect, it } from "vitest";
import { ApiError, type PairingStatus } from "../src/api.js";
import { clearWaiting, readWaiting, saveWaiting, untilDecided } from "../src/pair.js";

function memoryStorage(): Storage {
  const map = new Map<string, string>();
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
  } as unknown as Storage;
}

describe("pairing with the TV", () => {
  it("waits through pending and a dropped call, then hands over the approval", async () => {
    const answers: Array<PairingStatus | Error> = [
      { state: "pending" },
      new Error("offline"),
      { state: "approved", token: "phone-token", household: "tv-1", tv_name: "Living room", name: "Maya" },
    ];
    const seen: string[] = [];
    const api = {
      pairingStatus: async (token: string) => {
        seen.push(token);
        const next = answers.shift()!;
        if (next instanceof Error) throw next;
        return next;
      },
    };
    const out = await untilDecided(api, "waiting-token", { sleep: async () => {} });
    expect(out).toEqual({ state: "approved", token: "phone-token", household: "tv-1", tv_name: "Living room", name: "Maya" });
    expect(seen).toEqual(["waiting-token", "waiting-token", "waiting-token"]);
  });

  it("reports a turned-away phone and an expired request", async () => {
    const denied = { pairingStatus: async (): Promise<PairingStatus> => ({ state: "denied" }) };
    expect(await untilDecided(denied, "t", { sleep: async () => {} })).toEqual({ state: "denied" });
    const gone = {
      pairingStatus: async (): Promise<PairingStatus> => {
        throw new ApiError(401, "token expired");
      },
    };
    expect(await untilDecided(gone, "t", { sleep: async () => {} })).toEqual({ state: "expired" });
  });

  it("remembers the request for the same code across a reload, and the name for next time", () => {
    const storage = memoryStorage();
    saveWaiting(storage, { code: "4KB58PJP", token: "w", name: "Maya", tvName: "Living room" });
    expect(readWaiting(storage, "4KB58PJP")?.token).toBe("w");
    expect(readWaiting(storage, "OTHERCODE")).toBeNull();
    clearWaiting(storage);
    expect(readWaiting(storage, "4KB58PJP")).toBeNull();
    expect(storage.getItem("ml.pair.name")).toBe("Maya");
  });
});
