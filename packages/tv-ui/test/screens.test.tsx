import { fireEvent, render, screen, waitFor } from "@testing-library/react-native";
import { Ask } from "../src/screens/Ask";
import { Highlights } from "../src/screens/Highlights";
import { Invite } from "../src/screens/Invite";
import { JoinTrip } from "../src/screens/JoinTrip";
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

  it("MemoriesHome badges trips shared from another TV and lets you leave them", async () => {
    const api = new FakeApi();
    api.tripList = [{ ...trip, id: "lisbon", name: "Lisbon", shared_by: "Ruth's living room" }, trip];
    const onLeave = jest.fn(async () => undefined);
    await render(<MemoriesHome api={api} token="t" tripsVersion={0} onOpenTrip={() => undefined} onInvite={() => undefined} onJoin={() => undefined} onLeave={onLeave} />);
    await waitFor(() => expect(screen.getByTestId("shared-lisbon").props.children).toBe("Shared by Ruth's living room"));
    expect(screen.queryByTestId("shared-kyoto")).toBeNull();
    expect(screen.getByTestId("join")).toBeTruthy();
    await fireEvent.press(screen.getByTestId("leave-lisbon"));
    await waitFor(() => expect(screen.queryByTestId("trip-lisbon")).toBeNull());
    expect(onLeave).toHaveBeenCalled();
  });

  it("JoinTrip previews the code from the grid and joins", async () => {
    const api = new FakeApi();
    api.tripDetail = { ...trip, days: [], narrative: null, route: { points: [], inferred: false, map_key: null } };
    const onJoined = jest.fn();
    await render(<JoinTrip api={api} token="t" onJoined={onJoined} onBack={() => undefined} />);
    for (const c of ["T", "2", "C", "3", "D", "4"]) await fireEvent.press(screen.getByTestId(`key-${c}`));
    await waitFor(() => expect(screen.getByTestId("preview-name").props.children).toBe("Kyoto, April 2026"));
    expect(api.calls).toContain("preview T2C3D4");
    await fireEvent.press(screen.getByTestId("join"));
    await waitFor(() => expect(onJoined).toHaveBeenCalledWith("kyoto"));
    await fireEvent.press(screen.getByTestId("clear"));
    await fireEvent.changeText(screen.getByTestId("code"), "badbad");
    await waitFor(() => expect(screen.getByTestId("error")).toBeTruthy());
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
      contributor_roles: { owner: "owner", maya: "contributor" },
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
    await fireEvent.press(screen.getByTestId("promote-maya"));
    await waitFor(() => expect(api.calls).toContain("role kyoto maya co_editor"));
    await waitFor(() => expect(screen.queryByTestId("promote-maya")).toBeNull());
    await fireEvent.press(screen.getByTestId("regenerate"));
    expect(api.calls).toContain("regenerate kyoto");
    await waitFor(() => expect(screen.getByTestId("writing")).toBeTruthy());
  });

  it("TripView hides management on a joined trip", async () => {
    const api = new FakeApi();
    api.tripDetail = { ...trip, days: [], route: { points: [], inferred: false, map_key: null }, narrative: { text: "Story.", model_id: "m", generated_at: "" }, contributor_names: { owner: "Hanyu" }, contributor_roles: { owner: "owner" }, shared_by: "Hanyu's living room" };
    await render(<TripView api={api} token="t" tripId="kyoto" onBack={() => undefined} onInvite={() => undefined} canManage={false} />);
    await waitFor(() => expect(screen.getByTestId("shared-by").props.children).toBe("Shared by Hanyu's living room"));
    expect(screen.queryByTestId("regenerate")).toBeNull();
  });

  it("TripView keeps a withheld story off the screen", async () => {
    const api = new FakeApi();
    api.tripDetail = { ...trip, days: [], route: { points: [], inferred: false, map_key: null }, narrative: { text: "", model_id: "m", generated_at: "", check: "withheld" } };
    await render(<TripView api={api} token="t" tripId="kyoto" onBack={() => undefined} onInvite={() => undefined} />);
    await waitFor(() => expect(screen.queryByTestId("writing")).toBeNull());
    expect(screen.queryByTestId("narrative")).toBeNull();
  });

  it("Ask searches, shows matches, plays them, and admits when nothing matched", async () => {
    const api = new FakeApi();
    api.searchResults = [
      { media: "m4", score: 0.9, trip: "kyoto", contributor: "owner" },
      { media: "m9", score: 0.5, trip: null, contributor: "maya" },
    ];
    const onPlay = jest.fn();
    await render(<Ask api={api} token="t" onPlay={onPlay} onBack={() => undefined} />);
    await fireEvent.changeText(screen.getByTestId("query"), "the ramen place");
    await fireEvent.press(screen.getByTestId("search"));
    await waitFor(() => expect(screen.getByTestId("count").props.children).toBe("2 photos"));
    expect(api.calls).toContain("search the ramen place");
    expect(screen.getByTestId("result-m9")).toBeTruthy();
    await fireEvent.press(screen.getByTestId("play"));
    expect(onPlay).toHaveBeenCalledWith(expect.objectContaining({ id: "playlist:the ramen place", highlights: ["m4", "m9"] }));
    api.searchResults = [];
    await fireEvent.changeText(screen.getByTestId("query"), "snow");
    await fireEvent.press(screen.getByTestId("search"));
    await waitFor(() => expect(screen.getByTestId("nothing")).toBeTruthy());
  });

  it("Slideshow draws the answer card over the photo", async () => {
    const api = new FakeApi();
    const kyoto = { ...trip, days: [{ index: 1, date: "2026-04-03", media: ["m1"], title: "Day 1", route: { points: [], inferred: false, map_key: null } }], narrative: null, route: { points: [], inferred: false, map_key: null } };
    await render(<Slideshow api={api} token="t" trip={kyoto} onBack={() => undefined} answer={{ question: "what's that statue", answer: "The Great Buddha at Todai-ji." }} />);
    expect(screen.getByTestId("answer").props.children).toBe("The Great Buddha at Todai-ji.");
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

  it("ReviewChanges shows counts and samples, keeps everything or mints a token bound to the proposal", async () => {
    const api = new FakeApi();
    const samples = Array.from({ length: 6 }, (_, i) => ({ keep: `k${i}`, remove: [`r${i}`], reason: "near duplicate" }));
    api.proposalList = [
      { id: "p1", kind: "dedup", action: { action: "apply_dedup", group_ids: ["g1"] }, trip: "kyoto", before: 300, after: 288, samples, state: "open", requested_by: "agent", expires_at: null },
      { id: "p2", kind: "cleanup", action: { action: "apply_cleanup", media_ids: ["s1"] }, trip: null, before: 300, after: 299, samples: [{ keep: null, remove: ["s1"], reason: "screenshot" }], state: "open", requested_by: "agent", expires_at: null },
    ];
    await render(<ReviewChanges api={api} token="t" onDone={() => undefined} />);
    await waitFor(() => expect(screen.getByTestId("counts-p1").props.children).toBe("300 before · 288 after"));
    expect(screen.getByText("Remove 12 duplicates, keeping the best of each burst")).toBeTruthy();
    expect(screen.getByText("Tidy away 1 screenshot, receipts or blurry frames")).toBeTruthy();
    expect(screen.queryByTestId("sample-p1-4")).toBeNull();
    await fireEvent.press(screen.getByTestId("all-p1"));
    expect(screen.getByTestId("sample-p1-5")).toBeTruthy();
    expect(api.calls.filter((c) => c.startsWith("confirm"))).toHaveLength(0);
    await fireEvent.press(screen.getByTestId("cancel-p2"));
    await waitFor(() => expect(screen.queryByTestId("proposal-p2")).toBeNull());
    expect(api.calls).toContain("cancel p2");
    await fireEvent.press(screen.getByTestId("apply-p1"));
    await waitFor(() => expect(screen.queryByTestId("proposal-p1")).toBeNull());
    expect(api.calls).toContain("confirm apply_dedup p1");
    await waitFor(() => expect(screen.getByTestId("empty")).toBeTruthy());
  });
});
