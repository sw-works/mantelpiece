import { act, renderHook, waitFor } from "@testing-library/react-native";
import { BackHandler } from "react-native";
import { useDeviceSignIn } from "../src/hooks/useDeviceSignIn";
import { dayLabel, longDate } from "../src/api/client";
import { describe as describeEvent, reduce, useEvents } from "../src/hooks/useEvents";
import { useRemoteKeys } from "../src/hooks/useRemoteKeys";
import { FakeApi } from "./fakeApi";

const noSleep = async () => undefined;

describe("useDeviceSignIn", () => {
  it("shows the code while pending, then resolves the approval", async () => {
    const api = new FakeApi();
    api.polls = [{ state: "pending" }, { state: "pending" }, { state: "approved", household: "h", token: "tv-token" }];
    // Gate the retry sleep so the pending state is observable before approval lands.
    const gates: Array<() => void> = [];
    const sleep = () => new Promise<void>((r) => gates.push(r));
    const { result } = await renderHook(() => useDeviceSignIn({ api, deviceId: "tv-1", sleep }));
    await waitFor(() => expect(result.current.phase).toBe("waiting"));
    if (result.current.phase === "waiting") expect(result.current.code).toBe("AB2C3D4E");
    await waitFor(() => expect(gates.length).toBe(1));
    await act(async () => gates.shift()!());
    await waitFor(() => expect(gates.length).toBe(1));
    await act(async () => gates.shift()!());
    await waitFor(() => expect(result.current.phase).toBe("approved"));
    if (result.current.phase === "approved") expect(result.current.token).toBe("tv-token");
    expect(api.calls.filter((c) => c.startsWith("poll"))).toHaveLength(3);
  });

  it("reports expiry and restarts with a fresh code", async () => {
    const api = new FakeApi();
    api.polls = [{ state: "expired" }, { state: "approved", household: "h", token: "t" }];
    const { result } = await renderHook(() => useDeviceSignIn({ api, deviceId: "tv-1", tvName: "Living room", sleep: noSleep }));
    await waitFor(() => expect(result.current.phase).toBe("expired"));
    if (result.current.phase === "expired") expect(result.current.code).toBe("AB2C3D4E");
    await act(async () => result.current.restart());
    await waitFor(() => expect(result.current.phase).toBe("approved"));
    expect(api.calls.filter((c) => c.startsWith("issue"))).toEqual(["issue tv-1 Living room", "issue tv-1 Living room"]);
  });

  it("expires on the TV clock without asking the server", async () => {
    const api = new FakeApi();
    api.polls = [{ state: "pending" }];
    let clock = Date.now();
    const gates: Array<() => void> = [];
    const sleep = () => new Promise<void>((r) => gates.push(r));
    // Inline closures on every render must not restart the flow.
    const { result } = await renderHook(() => useDeviceSignIn({ api, deviceId: "tv-1", sleep: () => sleep(), now: () => clock }));
    await waitFor(() => expect(result.current.phase).toBe("waiting"));
    await waitFor(() => expect(gates.length).toBe(1));
    clock += api.expiresInMs + 1;
    await act(async () => gates.shift()!());
    await waitFor(() => expect(result.current.phase).toBe("expired"));
    expect(api.calls.filter((c) => c.startsWith("poll"))).toHaveLength(1);
  });
});

describe("events feed", () => {
  it("describes events for the ticker", () => {
    expect(describeEvent({ seq: 1, at: "", type: "contribution.received", display_name: "Maya", count: 42 })).toBe("Maya added 42 photos");
    expect(describeEvent({ seq: 1, at: "", type: "upload.progress", done: 3, total: 42 })).toBe("Uploading 3 of 42");
    expect(describeEvent({ seq: 1, at: "", type: "upload.progress", done: 42, total: 42 })).toBe("Uploaded 42, organising…");
    expect(describeEvent({ seq: 1, at: "", type: "membership.joined", member_name: "Ruth's living room" })).toBe("Ruth's living room joined");
    expect(describeEvent({ seq: 1, at: "", type: "cleanup.proposal", count: 7 })).toBe("7 photos to tidy up");
    expect(describeEvent({ seq: 1, at: "", type: "share.join_code", recipient: "Grandma", code: "T2C3D4" })).toBe("Grandma has a Mantelpiece TV: they can join with code T2C3D4");
    expect(describeEvent({ seq: 1, at: "", type: "reaction.received", actor_name: "Grandma Ruth", kind: "like", target: "day:3" })).toBe("Grandma Ruth loved day 3");
    expect(describeEvent({ seq: 1, at: "", type: "reaction.received", actor_name: "Maya", kind: "like", target: "media:m1" })).toBe("Maya loved a photo");
    expect(describeEvent({ seq: 1, at: "", type: "reaction.received", actor_name: "Grandma Ruth", kind: "comment", target: "media:m1", text: "That ramen!" })).toBe("Grandma Ruth: That ramen!");
    expect(describeEvent({ seq: 1, at: "", type: "unknown.thing" })).toBeNull();
  });

  it("reduces pages into recent, ticker and trips version", () => {
    const s0 = { since: 0, recent: [], ticker: null, tripsVersion: 0, play: null };
    const s1 = reduce(s0, [{ seq: 1, at: "", type: "upload.progress", done: 1, total: 3 }], 1);
    expect(s1).toMatchObject({ since: 1, ticker: "Uploading 1 of 3", tripsVersion: 0, play: null });
    const s2 = reduce(s1, [{ seq: 2, at: "", type: "trip.detected", trip: "t", new_media: 9 }], 2);
    expect(s2.tripsVersion).toBe(1);
    expect(reduce(s2, [], 2)).toBe(s2);
  });

  it("labels days and dates for the trip view", () => {
    expect(dayLabel({ index: 1, title: "Day 1" })).toBe("Day 1");
    expect(dayLabel({ index: 2, title: "Day 2: Kinkaku-ji" })).toBe("Day 2 · Kinkaku-ji");
    expect(dayLabel({ index: 3, title: null })).toBe("Day 3");
    expect(dayLabel({ index: 4, title: "Arashiyama" })).toBe("Day 4 · Arashiyama");
    expect(longDate("2025-03-25")).toBe("March 25, 2025");
    expect(longDate("2025,84")).toBe("2025,84");
  });

  it("bumps the phones version when a phone asks to connect", () => {
    const start = { since: 0, recent: [], ticker: null, tripsVersion: 0, play: null };
    const asked = reduce(start, [{ seq: 1, at: "2026-09-17T10:00:00Z", type: "phone.requested", contributor: "p1", display_name: "Maya" }], 1);
    expect(asked.phonesVersion).toBe(1);
    expect(asked.ticker).toBeNull();
    const other = reduce(asked, [{ seq: 2, at: "2026-09-17T10:01:00Z", type: "memory.ready", trip: "kyoto" }], 2);
    expect(other.phonesVersion).toBe(1);
  });

  it("tracks photos arriving for the home hero until the trip is detected", () => {
    const s0 = { since: 0, recent: [], ticker: null, tripsVersion: 0, play: null };
    const s1 = reduce(s0, [{ seq: 1, at: "", type: "upload.progress", done: 38, total: 120, display_name: "Maya" }], 1);
    expect(s1.upload).toEqual({ done: 38, total: 120, from: "Maya" });
    // An unrelated event keeps the last progress.
    const s2 = reduce(s1, [{ seq: 2, at: "", type: "reaction.received", actor_name: "Tom", kind: "like", target: "day:1" }], 2);
    expect(s2.upload).toEqual({ done: 38, total: 120, from: "Maya" });
    const s3 = reduce(s2, [{ seq: 3, at: "", type: "upload.progress", done: 120, total: 120 }], 3);
    expect(s3.upload).toEqual({ done: 120, total: 120, from: null });
    const s4 = reduce(s3, [{ seq: 4, at: "", type: "trip.detected", trip: "t", new_media: 120 }], 4);
    expect(s4.upload).toBeNull();
  });

  it("keeps the newest arrival per batch, with the size the phone announced", () => {
    const s0 = { since: 0, recent: [], ticker: null, tripsVersion: 0, play: null };
    const arrived = (seq: number, count: number, batch = "b1") => ({ seq, at: "2026-09-21T10:00:00Z", type: "photos.arrived", contributor: "maya", display_name: "Maya", batch, media: Array.from({ length: count }, (_, i) => `m${i}`), count });
    const s1 = reduce(s0, [{ seq: 1, at: "", type: "upload.progress", contributor: "maya", done: 1, total: 14 }, arrived(2, 1)], 2);
    expect(s1.arrival).toMatchObject({ batch: "b1", from: "Maya", count: 1, expected: 14, media: ["m0"] });
    expect(s1.ticker).toBe("Maya sent 1 photo");
    // Parallel Signal runs can announce out of order; the larger count wins.
    const s2 = reduce(s1, [arrived(3, 3), arrived(4, 2)], 4);
    expect(s2.arrival?.count).toBe(3);
    const s3 = reduce(s2, [arrived(5, 1, "b2")], 5);
    expect(s3.arrival).toMatchObject({ batch: "b2", count: 1 });
  });

  it("tracks the live broadcast until it ends", () => {
    const s0 = { since: 0, recent: [], ticker: null, tripsVersion: 0, play: null };
    const s1 = reduce(s0, [{ seq: 1, at: "2026-09-21T10:00:00Z", type: "live.started", broadcast: "l1", display_name: "Maya", playback_url: "https://p/x.m3u8" }], 1);
    expect(s1.live).toEqual({ seq: 1, at: "2026-09-21T10:00:00Z", id: "l1", from: "Maya", url: "https://p/x.m3u8" });
    expect(s1.ticker).toBe("Maya is live");
    // An end for some other broadcast changes nothing.
    const s2 = reduce(s1, [{ seq: 2, at: "", type: "live.ended", broadcast: "l0" }], 2);
    expect(s2.live?.id).toBe("l1");
    const s3 = reduce(s2, [{ seq: 3, at: "", type: "live.ended", broadcast: "l1" }], 3);
    expect(s3.live).toBeNull();
  });

  it("keeps the newest play request and its ticker line", () => {
    const s0 = { since: 0, recent: [], ticker: null, tripsVersion: 0, play: null };
    const s1 = reduce(
      s0,
      [
        { seq: 1, at: "", type: "play.requested", trip: "kyoto", playlist: null, name: "Kyoto", source: "alexa" },
        { seq: 2, at: "", type: "play.requested", trip: null, playlist: "p1", name: "Beach days", source: "agent" },
      ],
      2,
    );
    expect(s1.play).toEqual({ seq: 2, trip: null, playlist: "p1", name: "Beach days", source: "agent", start: null });
    expect(s1.ticker).toBe("Playing Beach days");
    const s2 = reduce(s1, [{ seq: 3, at: "", type: "memory.ready", trip: "t" }], 3);
    expect(s2.play).toBe(s1.play);
    // "Play this photo on the TV" from the phone carries the media id to open at.
    const s3 = reduce(s2, [{ seq: 4, at: "", type: "play.requested", trip: "kyoto", playlist: null, name: "Kyoto", source: "phone", start: "m7" }], 4);
    expect(s3.play).toEqual({ seq: 4, trip: "kyoto", playlist: null, name: "Kyoto", source: "phone", start: "m7" });
  });

  it("long-polls and advances since", async () => {
    const api = new FakeApi();
    api.eventPages = [
      { events: [{ seq: 1, at: "", type: "upload.progress", done: 1, total: 1 }], next: 1 },
      { events: [{ seq: 2, at: "", type: "memory.ready", trip: "t" }], next: 2 },
    ];
    const { result, unmount } = await renderHook(() => useEvents(api, "tok", { sleep: noSleep }));
    await waitFor(() => expect(result.current.since).toBe(2));
    expect(result.current.ticker).toBe("Memory ready");
    expect(api.calls.slice(0, 3)).toEqual(["events 0", "events 1", "events 2"]);
    await unmount();
  });
});

describe("useRemoteKeys", () => {
  it("consumes hardware Back only when a handler is given", async () => {
    let pressed: (() => boolean | null | undefined) | null = null;
    const remove = jest.fn();
    jest.spyOn(BackHandler, "addEventListener").mockImplementation((_name, handler) => {
      pressed = handler as () => boolean;
      return { remove };
    });
    const onBack = jest.fn();
    const { rerender, unmount } = await renderHook((props: { onBack?: () => void }) => useRemoteKeys(props), { initialProps: { onBack } });
    expect(pressed!()).toBe(true);
    expect(onBack).toHaveBeenCalledTimes(1);
    await rerender({});
    expect(pressed!()).toBe(false);
    await unmount();
    expect(remove).toHaveBeenCalled();
  });
});
