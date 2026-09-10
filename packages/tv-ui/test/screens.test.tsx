import { fireEvent, render, screen, waitFor } from "@testing-library/react-native";
import { Highlights } from "../src/screens/Highlights";
import { Invite } from "../src/screens/Invite";
import { MemoriesHome } from "../src/screens/MemoriesHome";
import { Onboarding } from "../src/screens/Onboarding";
import { ReviewChanges } from "../src/screens/ReviewChanges";
import { Slideshow } from "../src/screens/Slideshow";
import { TripView } from "../src/screens/TripView";
import { FakeApi } from "./fakeApi";

const trip = {
  id: "kyoto",
  name: "Kyoto, April 2026",
  start: "2026-04-03T00:00:00Z",
  end: "2026-04-09T00:00:00Z",
  hero: "m1",
  highlights: ["m1"],
  contributors: ["owner", "maya"],
};

describe("screens", () => {
  it("Onboarding shows the code and QR, then hands over the session", async () => {
    const api = new FakeApi();
    api.polls = [{ state: "approved", household: "h", token: "t" }];
    const onSignedIn = jest.fn();
    await render(<Onboarding api={api} deviceId="tv-1" onSignedIn={onSignedIn} retryMs={0} />);
    await waitFor(() => expect(onSignedIn).toHaveBeenCalledWith({ token: "t", household: "h" }));
  });

  it("Onboarding renders the code while waiting", async () => {
    const api = new FakeApi();
    api.polls = [{ state: "pending" }];
    await render(<Onboarding api={api} deviceId="tv-1" onSignedIn={() => undefined} retryMs={50} />);
    await waitFor(() => expect(screen.getByTestId("device-code").props.children).toBe("AB2C 3D4E"));
    expect(screen.getByTestId("qr")).toBeTruthy();
  });

  it("Onboarding offers a new code after expiry", async () => {
    const api = new FakeApi();
    api.polls = [{ state: "expired" }, { state: "pending" }];
    await render(<Onboarding api={api} deviceId="tv-1" onSignedIn={() => undefined} retryMs={50} />);
    await waitFor(() => expect(screen.getByTestId("expired")).toBeTruthy());
    await fireEvent.press(screen.getByText("Get a new code"));
    await waitFor(() => expect(screen.getByTestId("device-code")).toBeTruthy());
    expect(api.calls.filter((c) => c.startsWith("issue"))).toHaveLength(2);
  });

  it("MemoriesHome lists trips with a new badge and opens one", async () => {
    const api = new FakeApi();
    api.tripList = [trip];
    const onOpen = jest.fn();
    await render(<MemoriesHome api={api} token="t" tripsVersion={0} onOpenTrip={onOpen} onInvite={() => undefined} />);
    await waitFor(() => expect(screen.getByTestId("trip-kyoto")).toBeTruthy());
    expect(screen.getByTestId("new-kyoto")).toBeTruthy();
    expect(screen.getByText("2026-04-03 to 2026-04-09 · 2 contributors")).toBeTruthy();
    await fireEvent.press(screen.getByText("Open"));
    expect(onOpen).toHaveBeenCalledWith(trip);
    await waitFor(() => expect(screen.queryByTestId("new-kyoto")).toBeNull());
  });

  it("MemoriesHome explains the empty state", async () => {
    await render(<MemoriesHome api={new FakeApi()} token="t" tripsVersion={0} onOpenTrip={() => undefined} onInvite={() => undefined} />);
    await waitFor(() => expect(screen.getByTestId("empty")).toBeTruthy());
  });

  it("TripView shows days, narrative, and routes Invite to the trip", async () => {
    const api = new FakeApi();
    api.tripDetail = {
      ...trip,
      days: [
        { index: 1, date: "2026-04-03", media: ["m1", "m2"], title: "Day 1: Kinkaku-ji", route: { points: [], inferred: false, map_key: "maps/h/kyoto/day-1.png" }, story: "The golden day." },
        { index: 2, date: "2026-04-04", media: ["m3"], title: "Day 2", route: { points: [], inferred: true, map_key: null }, undated: ["m3"] },
      ],
      narrative: { text: "We walked the golden pavilion.", model_id: "m", generated_at: "", check: "passed" },
      route: { points: [], inferred: false, map_key: null },
      contributor_names: { owner: "Hanyu", maya: "Maya" },
      media_credits: { m1: "owner", m2: "maya", m3: "maya" },
      clock_offsets: [{ contributor: "maya", seconds: -184, samples: 6 }, { contributor: "owner", seconds: 2, samples: 9 }],
    };
    const onInvite = jest.fn();
    const onHighlights = jest.fn();
    const onPlay = jest.fn();
    await render(<TripView api={api} token="t" tripId="kyoto" onBack={() => undefined} onInvite={onInvite} onHighlights={onHighlights} onPlay={onPlay} />);
    await waitFor(() => expect(screen.getByTestId("narrative")).toBeTruthy());
    expect(screen.getByTestId("narrative").props.children).toBe("The golden day.");
    expect(screen.getByTestId("day-summary").props.children).toBe("2026-04-03 · 2 photos from Hanyu, Maya");
    expect(screen.getByTestId("credit-m2").props.children).toBe("Maya");
    expect(screen.getByTestId("clock-hints").props.children).toBe("Maya's clock corrected by 3 min");
    await waitFor(() => expect(screen.getByTestId("map")).toBeTruthy());
    expect(api.calls).toContain("map kyoto day 1");
    await fireEvent.press(screen.getByTestId("day-2"));
    expect(screen.getByTestId("day-summary").props.children).toBe("2026-04-04 · 1 photos from Maya · route inferred · 1 undated");
    expect(screen.getByTestId("narrative").props.children).toBe("We walked the golden pavilion.");
    await fireEvent.press(screen.getByTestId("invite"));
    expect(onInvite).toHaveBeenCalledWith("kyoto");
    await fireEvent.press(screen.getByTestId("highlights"));
    expect(onHighlights).toHaveBeenCalledWith("kyoto");
    await fireEvent.press(screen.getByTestId("play"));
    expect(onPlay).toHaveBeenCalledWith(expect.objectContaining({ id: "kyoto" }));
    await fireEvent.press(screen.getByTestId("regenerate"));
    expect(api.calls).toContain("regenerate kyoto");
    await waitFor(() => expect(screen.getByTestId("writing")).toBeTruthy());
  });

  it("TripView keeps a withheld story off the screen", async () => {
    const api = new FakeApi();
    api.tripDetail = { ...trip, days: [], route: { points: [], inferred: false, map_key: null }, narrative: { text: "", model_id: "m", generated_at: "", check: "withheld" } };
    await render(<TripView api={api} token="t" tripId="kyoto" onBack={() => undefined} onInvite={() => undefined} />);
    await waitFor(() => expect(screen.queryByTestId("writing")).toBeNull());
    expect(screen.queryByTestId("narrative")).toBeNull();
  });

  it("Slideshow advances, pauses, jumps days and reports presence", async () => {
    const api = new FakeApi();
    const kyoto = {
      ...trip,
      days: [
        { index: 1, date: "2026-04-03", media: ["m1", "m2"], title: "Day 1", route: { points: [], inferred: false, map_key: null } },
        { index: 2, date: "2026-04-04", media: ["m3"], title: "Day 2", route: { points: [], inferred: false, map_key: null } },
      ],
      narrative: null,
      route: { points: [], inferred: false, map_key: null },
    };
    const presence: Array<{ media: string; day: number; paused: boolean }> = [];
    await render(<Slideshow api={api} token="t" trip={kyoto} onBack={() => undefined} onPresence={(p) => presence.push(p)} intervalMs={30} />);
    expect(screen.getByTestId("caption").props.children).toBe("Day 1 · 1 of 3");
    await waitFor(() => expect(screen.getByTestId("caption").props.children).toBe("Day 1 · 2 of 3"));
    await fireEvent.press(screen.getByTestId("playpause"));
    expect(screen.getByTestId("caption").props.children).toBe("Day 1 · 2 of 3 · paused");
    await fireEvent.press(screen.getByTestId("next-day"));
    expect(screen.getByTestId("caption").props.children).toBe("Day 2 · 3 of 3 · paused");
    await fireEvent.press(screen.getByTestId("prev-day"));
    expect(screen.getByTestId("caption").props.children).toBe("Day 1 · 1 of 3 · paused");
    expect(presence.at(-1)).toEqual({ media: "m1", day: 1, paused: true });
    expect(presence.some((p) => p.media === "m3" && p.day === 2)).toBe(true);
  });

  it("Highlights shows the trip's picks with credits", async () => {
    const api = new FakeApi();
    api.tripDetail = {
      ...trip,
      highlights: ["m1", "m2"],
      days: [],
      narrative: null,
      route: { points: [], inferred: false, map_key: null },
      contributor_names: { owner: "Hanyu" },
      media_credits: { m1: "owner" },
    };
    await render(<Highlights api={api} token="t" tripId="kyoto" onBack={() => undefined} />);
    await waitFor(() => expect(screen.getByTestId("highlight-m1")).toBeTruthy());
    expect(screen.getByTestId("highlight-m2")).toBeTruthy();
    expect(screen.getByText("Hanyu")).toBeTruthy();
    await fireEvent.press(screen.getByTestId("keeper-m2"));
    await waitFor(() => expect(api.calls).toContain("keeper m2"));
  });

  it("Invite asks for a trip code when scoped and shows the ticker", async () => {
    const api = new FakeApi();
    await render(<Invite api={api} token="t" tripId="kyoto" ticker="Maya added 42 photos" onBack={() => undefined} />);
    await waitFor(() => expect(screen.getByTestId("code").props.children).toBe("T2C3D4"));
    expect(api.calls).toContain("tripCode kyoto");
    expect(screen.getByTestId("ticker")).toBeTruthy();
    expect(screen.getByTestId("qr")).toBeTruthy();
    expect(screen.getByTestId("counts").props.children).toBe("3 contributors · 1 households watching");
    await fireEvent.press(screen.getByTestId("rotate"));
    await waitFor(() => expect(screen.getByTestId("code").props.children).toBe("R2C3D4"));
    expect(api.calls).toContain("rotate T2C3D4");
  });

  it("ReviewChanges mints a confirmation only when the user applies", async () => {
    const api = new FakeApi();
    const onConfirmed = jest.fn();
    const proposal = { title: "Remove 12 duplicates", before: 300, after: 288, action: { action: "apply_dedup", group_ids: ["g1"] }, sample: [] };
    await render(<ReviewChanges api={api} token="t" proposal={proposal} onConfirmed={onConfirmed} onCancel={() => undefined} />);
    expect(screen.getByTestId("counts").props.children).toBe("300 before · 288 after");
    expect(api.calls).toHaveLength(0);
    await fireEvent.press(screen.getByTestId("apply"));
    await waitFor(() => expect(onConfirmed).toHaveBeenCalledWith("conf-1"));
    expect(api.calls).toEqual(["confirm apply_dedup"]);
  });
});
