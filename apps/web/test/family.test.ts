import { describe, expect, it } from "vitest";
import { ago, base64UrlToBytes, describeNote, phoneToken, pushSupport, type PushEnv } from "../src/family.js";
import { cameraConstraints, holdHint, liveErrorText } from "../src/live.js";

const android: PushEnv = { hasServiceWorker: true, hasPushManager: true, permission: "default", userAgent: "Mozilla/5.0 (Linux; Android 15) Chrome/140", standalone: false };

function memoryStorage(entries: Record<string, string> = {}): Storage {
  const m = new Map(Object.entries(entries));
  return {
    get length() {
      return m.size;
    },
    clear: () => m.clear(),
    getItem: (k) => m.get(k) ?? null,
    key: (i) => [...m.keys()][i] ?? null,
    removeItem: (k) => void m.delete(k),
    setItem: (k, v) => void m.set(k, v),
  };
}

describe("family channel on the phone", () => {
  it("offers notifications on Android and asks iPhones to install first", () => {
    expect(pushSupport(android)).toBe("ready");
    const iphone = { ...android, userAgent: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Safari/604.1" };
    expect(pushSupport(iphone)).toBe("install");
    expect(pushSupport({ ...iphone, standalone: true })).toBe("ready");
    expect(pushSupport({ ...android, permission: "denied" })).toBe("denied");
    expect(pushSupport({ ...android, hasPushManager: false })).toBe("unsupported");
    expect(pushSupport({ ...android, permission: null })).toBe("unsupported");
  });

  it("turns the VAPID key into the bytes the Push API wants", () => {
    const bytes = base64UrlToBytes("BPk_-w");
    expect(Array.from(bytes)).toEqual([0x04, 0xf9, 0x3f, 0xfb]);
  });

  it("says who loved what, and when, in plain words", () => {
    expect(describeNote({ at: "2026-09-21T10:00:00Z", from: "Grandma's TV", kind: "love", count: 14 })).toBe("Grandma's TV loved your 14 photos");
    expect(describeNote({ at: "2026-09-21T10:00:00Z", from: "Den", kind: "love", count: 1 })).toBe("Den loved your 1 photo");
    const t = Date.parse("2026-09-21T10:00:00Z");
    expect(ago("2026-09-21T10:00:00Z", t + 20_000)).toBe("just now");
    expect(ago("2026-09-21T10:00:00Z", t + 5 * 60_000)).toBe("5 min ago");
    expect(ago("2026-09-21T10:00:00Z", t + 3 * 3600_000)).toBe("3 h ago");
    expect(ago("2026-09-21T10:00:00Z", t + 26 * 3600_000)).toBe("yesterday");
  });

  it("finds the phone's session, paired or guest", () => {
    expect(phoneToken(memoryStorage({ "ml.owner.token": "paired" }))).toBe("paired");
    expect(phoneToken(memoryStorage({ "ml.upload.session": JSON.stringify({ token: "guest" }) }))).toBe("guest");
    expect(phoneToken(memoryStorage({ "ml.upload.session": "{not json" }))).toBeNull();
    expect(phoneToken(memoryStorage())).toBeNull();
  });
});

describe("going live", () => {
  it("asks for a 720p back camera with voice cleanup", () => {
    const c = cameraConstraints("environment");
    expect(c.video).toMatchObject({ facingMode: "environment", width: { ideal: 1280 } });
    expect(c.audio).toMatchObject({ echoCancellation: true });
    expect((cameraConstraints("user").video as MediaTrackConstraints).facingMode).toBe("user");
  });

  it("suggests landscape only when the phone is upright", () => {
    expect(holdHint(390, 844)).toMatch(/sideways/);
    expect(holdHint(844, 390)).toBeNull();
  });

  it("explains refused cameras and unpaired phones", () => {
    const refused = Object.assign(new Error("denied"), { name: "NotAllowedError" });
    expect(liveErrorText(refused)).toMatch(/refused/);
    expect(liveErrorText(new Error("403 this phone is no longer paired"))).toMatch(/not paired/);
    expect(liveErrorText(new Error("boom"))).toBe("Could not go live: boom");
  });
});
