import { fireEvent, render, screen, waitFor } from "@testing-library/react-native";
import { Invite } from "../src/screens/Invite";
import { MemoriesHome } from "../src/screens/MemoriesHome";
import { Onboarding } from "../src/screens/Onboarding";
import { ReviewChanges } from "../src/screens/ReviewChanges";
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
        { index: 1, date: "2026-04-03", media: ["m1", "m2"], title: "Day 1: Kinkaku-ji", route: { points: [], inferred: false, map_key: null } },
        { index: 2, date: "2026-04-04", media: ["m3"], title: "Day 2", route: { points: [], inferred: true, map_key: null } },
      ],
      narrative: { text: "We walked the golden pavilion.", model_id: "m", generated_at: "" },
      route: { points: [], inferred: false, map_key: null },
    };
    const onInvite = jest.fn();
    await render(<TripView api={api} token="t" tripId="kyoto" onBack={() => undefined} onInvite={onInvite} />);
    await waitFor(() => expect(screen.getByTestId("narrative")).toBeTruthy());
    await fireEvent.press(screen.getByTestId("day-2"));
    expect(screen.getByTestId("day-summary").props.children).toBe("2026-04-04 · 1 photos · route inferred");
    await fireEvent.press(screen.getByTestId("invite"));
    expect(onInvite).toHaveBeenCalledWith("kyoto");
  });

  it("Invite asks for a trip code when scoped and shows the ticker", async () => {
    const api = new FakeApi();
    await render(<Invite api={api} token="t" tripId="kyoto" ticker="Maya added 42 photos" onBack={() => undefined} />);
    await waitFor(() => expect(screen.getByTestId("code").props.children).toBe("T2C3D4"));
    expect(api.calls).toContain("tripCode kyoto");
    expect(screen.getByTestId("ticker")).toBeTruthy();
    expect(screen.getByTestId("qr")).toBeTruthy();
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
