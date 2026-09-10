import { act, renderHook, waitFor } from "@testing-library/react-native";
import { BackHandler } from "react-native";
import { useDeviceSignIn } from "../src/hooks/useDeviceSignIn";
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
    expect(describeEvent({ seq: 1, at: "", type: "reaction.received", actor_name: "Grandma Ruth", kind: "like", target: "day:3" })).toBe("Grandma Ruth loved day 3");
    expect(describeEvent({ seq: 1, at: "", type: "reaction.received", actor_name: "Maya", kind: "like", target: "media:m1" })).toBe("Maya loved a photo");
    expect(describeEvent({ seq: 1, at: "", type: "reaction.received", actor_name: "Grandma Ruth", kind: "comment", target: "media:m1", text: "That ramen!" })).toBe("Grandma Ruth: That ramen!");
    expect(describeEvent({ seq: 1, at: "", type: "unknown.thing" })).toBeNull();
  });

  it("reduces pages into recent, ticker and trips version", () => {
    const s0 = { since: 0, recent: [], ticker: null, tripsVersion: 0 };
    const s1 = reduce(s0, [{ seq: 1, at: "", type: "upload.progress", done: 1, total: 3 }], 1);
    expect(s1).toMatchObject({ since: 1, ticker: "Uploading 1 of 3", tripsVersion: 0 });
    const s2 = reduce(s1, [{ seq: 2, at: "", type: "trip.detected", trip: "t", new_media: 9 }], 2);
    expect(s2.tripsVersion).toBe(1);
    expect(reduce(s2, [], 2)).toBe(s2);
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
