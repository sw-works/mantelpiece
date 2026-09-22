import { act, fireEvent, render, screen, waitFor } from "@testing-library/react-native";
import { useEffect } from "react";
import { Text } from "react-native";
import { App } from "../src/App";
import { type Arrival, type LiveNow } from "../src/hooks/useEvents";
import { type LivePlayerProps } from "../src/live";
import { Arrived, arrivalCount } from "../src/screens/Arrived";
import { Live } from "../src/screens/Live";
import { MemorySessionStore } from "../src/session";
import { FakeApi } from "./fakeApi";

const now = () => new Date().toISOString();

const arrival = (over: Partial<Arrival> = {}): Arrival => ({
  seq: 5,
  at: now(),
  batch: "b1",
  contributor: "maya",
  from: "Maya",
  media: ["m1", "m2", "m3"],
  count: 3,
  expected: 14,
  ...over,
});

const broadcast: LiveNow = { seq: 9, at: now(), id: "l1", from: "Maya", url: "https://play.test/l1.m3u8" };

function FakePlayer({ uri, onState }: LivePlayerProps) {
  onState?.("playing");
  return <Text testID="player">{uri}</Text>;
}

describe("family channel on the TV", () => {
  it("counts the batch the phone announced, not only what landed", () => {
    expect(arrivalCount(arrival())).toBe(14);
    expect(arrivalCount(arrival({ expected: null }))).toBe(3);
    expect(arrivalCount(arrival({ count: 20, expected: 14 }))).toBe(20);
  });

  it("Arrived shows who sent what and loves it with one button", async () => {
    const api = new FakeApi();
    const onClose = jest.fn();
    await render(<Arrived api={api} token="t" arrival={arrival()} onPlayAll={jest.fn()} onClose={onClose} idleCloseMs={0} />);
    expect(screen.getByTestId("arrived-title").props.children).toBe("From Maya");
    expect(screen.getByTestId("arrived-meta").props.children).toBe("14 photos · 1 of 3 on screen");
    await fireEvent.press(screen.getByTestId("love"));
    await waitFor(() => expect(api.calls).toContain("love maya 14 m1"));
    await waitFor(() => expect(screen.getByText("Loved · Maya will see it")).toBeTruthy());
    await fireEvent.press(screen.getByTestId("close"));
    expect(onClose).toHaveBeenCalled();
  });

  it("Arrived offers a retry when the love does not go through, and plays everything", async () => {
    const api = new FakeApi();
    api.loveError = new Error("offline");
    const onPlayAll = jest.fn();
    await render(<Arrived api={api} token="t" arrival={arrival()} onPlayAll={onPlayAll} onClose={jest.fn()} idleCloseMs={0} />);
    await fireEvent.press(screen.getByTestId("love"));
    await waitFor(() => expect(screen.getByText("Try again")).toBeTruthy());
    await fireEvent.press(screen.getByTestId("play-all"));
    expect(onPlayAll).toHaveBeenCalledWith(["m1", "m2", "m3"], "Maya");
  });

  it("Arrived goes back on its own after a quiet spell", async () => {
    jest.useFakeTimers();
    try {
      const onClose = jest.fn();
      await render(<Arrived api={new FakeApi()} token="t" arrival={arrival()} onPlayAll={jest.fn()} onClose={onClose} idleCloseMs={1000} />);
      await act(async () => {
        jest.advanceTimersByTime(1100);
      });
      expect(onClose).toHaveBeenCalled();
    } finally {
      jest.useRealTimers();
    }
  });

  it("Live plays the broadcast with who and how far behind, and ends it on request", async () => {
    const onEnd = jest.fn();
    const { rerender } = await render(<Live live={broadcast} from="Maya" player={FakePlayer} onBack={jest.fn()} onEnd={onEnd} endedCloseMs={0} />);
    expect(screen.getByTestId("player").props.children).toBe("https://play.test/l1.m3u8");
    expect(screen.getByTestId("live-who").props.children).toBe("Live · Maya");
    await waitFor(() => expect(screen.getByTestId("live-state").props.children).toBe("A few seconds behind the room"));
    await fireEvent.press(screen.getByTestId("end"));
    expect(onEnd).toHaveBeenCalled();
    // Ended from the TV: nobody on the phone did it.
    await rerender(<Live live={null} from="Maya" player={FakePlayer} onBack={jest.fn()} onEnd={onEnd} endedCloseMs={0} />);
    expect(screen.getByText("Broadcast ended")).toBeTruthy();
  });

  it("Live says when the phone stopped and without a player explains itself", async () => {
    const onBack = jest.fn();
    const { rerender } = await render(<Live live={broadcast} from="Maya" onBack={onBack} onEnd={jest.fn()} endedCloseMs={0} />);
    expect(screen.getByTestId("no-player")).toBeTruthy();
    await rerender(<Live live={null} from="Maya" onBack={onBack} onEnd={jest.fn()} endedCloseMs={0} />);
    expect(screen.getByText("Maya ended the broadcast")).toBeTruthy();
  });

  it("App lets new photos take over Memories home, then a broadcast take over the photos", async () => {
    const api = new FakeApi();
    const store = new MemorySessionStore();
    await store.save({ token: "tv-token", household: "tv-house", deviceId: "tv-1" });
    api.eventPages = [
      { events: [{ seq: 1, at: now(), type: "photos.arrived", contributor: "maya", display_name: "Maya", batch: "b1", media: ["m1"], count: 1 }], next: 1 },
    ];
    await render(<App api={api} sessionStore={store} idleMs={0} livePlayer={FakePlayer} />);
    await waitFor(() => expect(screen.getByTestId("arrived")).toBeTruthy());
    api.eventPages.push({ events: [{ seq: 2, at: now(), type: "live.started", broadcast: "l1", display_name: "Maya", playback_url: "https://play.test/l1.m3u8" }], next: 2 });
    await waitFor(() => expect(screen.getByTestId("player")).toBeTruthy());
    api.eventPages.push({ events: [{ seq: 3, at: now(), type: "live.ended", broadcast: "l1" }], next: 3 });
    await waitFor(() => expect(screen.getByText("Maya ended the broadcast")).toBeTruthy());
  });

  it("App leaves old arrivals from a cold start's replay alone", async () => {
    const api = new FakeApi();
    const store = new MemorySessionStore();
    await store.save({ token: "tv-token", household: "tv-house", deviceId: "tv-1" });
    const old = new Date(Date.now() - 60 * 60 * 1000).toISOString();
    api.eventPages = [{ events: [{ seq: 1, at: old, type: "photos.arrived", contributor: "maya", display_name: "Maya", batch: "b1", media: ["m1"], count: 1 }], next: 1 }];
    await render(<App api={api} sessionStore={store} idleMs={0} />);
    await waitFor(() => expect(api.calls).toContain("events 1"));
    expect(screen.queryByTestId("arrived")).toBeNull();
  });

  it("App asks before a phone goes live, and a grant opens the broadcast", async () => {
    const api = new FakeApi();
    const store = new MemorySessionStore();
    await store.save({ token: "tv-token", household: "tv-house", deviceId: "tv-1" });
    const soon = new Date(Date.now() + 60_000).toISOString();
    api.eventPages = [{ events: [{ seq: 1, at: now(), type: "live.requested", request: "r1", display_name: "Maya", expires_at: soon }], next: 1 }];
    await render(<App api={api} sessionStore={store} idleMs={0} livePlayer={FakePlayer} />);
    await waitFor(() => expect(screen.getByTestId("live-request-title").props.children).toBe("Maya wants to go live"));
    // Nothing plays until someone here says yes.
    expect(screen.queryByTestId("player")).toBeNull();
    await fireEvent.press(screen.getByTestId("live-grant"));
    expect(api.calls).toContain("grant r1");
    await waitFor(() => expect(screen.queryByTestId("live-request")).toBeNull());
    api.eventPages.push({
      events: [
        { seq: 2, at: now(), type: "live.request_closed", request: "r1" },
        { seq: 3, at: now(), type: "live.started", broadcast: "l1", display_name: "Maya", playback_url: "https://play.test/l1.m3u8" },
      ],
      next: 3,
    });
    await waitFor(() => expect(screen.getByTestId("player")).toBeTruthy());
  });

  it("App says whose broadcast a switch ends, and Not now leaves it playing", async () => {
    const api = new FakeApi();
    const store = new MemorySessionStore();
    await store.save({ token: "tv-token", household: "tv-house", deviceId: "tv-1" });
    const soon = new Date(Date.now() + 60_000).toISOString();
    api.eventPages = [
      {
        events: [
          { seq: 1, at: now(), type: "live.started", broadcast: "l1", display_name: "Maya", playback_url: "https://play.test/l1.m3u8" },
          { seq: 2, at: now(), type: "live.requested", request: "r2", display_name: "Sam", expires_at: soon, replacing: "Maya" },
          { seq: 3, at: now(), type: "live.requested", request: "old", display_name: "Ben", expires_at: new Date(Date.now() - 1000).toISOString() },
        ],
        next: 3,
      },
    ];
    await render(<App api={api} sessionStore={store} idleMs={0} livePlayer={FakePlayer} />);
    await waitFor(() => expect(screen.getByTestId("live-request-title").props.children).toBe("Sam wants to go live"));
    expect(screen.getByTestId("live-request-meta").props.children).toBe("This ends Maya's broadcast.");
    expect(screen.getByText("Switch to Sam")).toBeTruthy();
    expect(screen.getByTestId("player").props.children).toBe("https://play.test/l1.m3u8");
    await fireEvent.press(screen.getByTestId("live-decline"));
    expect(api.calls).toContain("decline r2");
    // Ben's request ran out before the TV saw it: no prompt for it.
    await waitFor(() => expect(screen.queryByTestId("live-request")).toBeNull());
    expect(screen.getByTestId("player").props.children).toBe("https://play.test/l1.m3u8");
  });

  it("Live builds the player again when the picture stops, then gives up", async () => {
    // A player that always fails: every fresh one reports the wedged pipeline.
    let mounts = 0;
    function BrokenPlayer({ uri, onState }: LivePlayerProps) {
      useEffect(() => {
        mounts += 1;
        onState?.("error");
      }, [onState]);
      return <Text testID="player">{uri}</Text>;
    }
    jest.useFakeTimers();
    try {
      await render(<Live live={broadcast} from="Maya" player={BrokenPlayer} onBack={jest.fn()} onEnd={jest.fn()} endedCloseMs={0} retryMs={10} />);
      expect(screen.getByTestId("live-state").props.children).toMatch(/Trying again/);
      for (let i = 0; i < 5; i++) {
        await act(async () => {
          jest.advanceTimersByTime(20);
        });
      }
      // The first player plus three retries, no more, and then it says what to do.
      expect(mounts).toBe(4);
      expect(screen.getByTestId("live-state").props.children).toMatch(/Press Back/);
    } finally {
      jest.useRealTimers();
    }
  });
});
