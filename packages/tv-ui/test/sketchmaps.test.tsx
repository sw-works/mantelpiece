import { render, screen, waitFor } from "@testing-library/react-native";
import { Slideshow } from "../src/screens/Slideshow";
import { FakeApi } from "./fakeApi";

const point = { at: { lat: 35.0, lon: 135.7 }, media: "m1" };
const trip = {
  id: "kyoto",
  name: "Kyoto, April 2026",
  start: "2026-04-03T00:00:00Z",
  end: "2026-04-09T00:00:00Z",
  hero: "m1",
  highlights: ["m1"],
  contributors: ["owner"],
  days: [
    { index: 1, date: "2026-04-03", media: ["m1"], title: "Day 1", route: { points: [point], inferred: false, map_key: null } },
    { index: 2, date: "2026-04-04", media: ["m2"], title: "Day 2", route: { points: [], inferred: false, map_key: null } },
  ],
  narrative: null,
  route: { points: [], inferred: false, map_key: null },
};

describe("sketch maps in the slideshow (F-712)", () => {
  it("opens a day that has a route on its map, then its first photo", async () => {
    const api = new FakeApi();
    await render(<Slideshow api={api} token="t" trip={trip} onBack={() => undefined} intervalMs={100000} dayMapMs={50} />);
    expect(screen.getByTestId("day-map")).toBeTruthy();
    await waitFor(() => expect(api.calls).toContain("map kyoto day 1"));
    await waitFor(() => expect(screen.queryByTestId("day-map")).toBeNull());
  });

  it("goes straight to the photo on a day with no route, or when day maps are off", async () => {
    const api = new FakeApi();
    await render(<Slideshow api={api} token="t" trip={trip} onBack={() => undefined} intervalMs={100000} dayMapMs={0} />);
    expect(screen.queryByTestId("day-map")).toBeNull();
    const noRoute = { ...trip, days: [trip.days[1]!] };
    await render(<Slideshow api={api} token="t" trip={noRoute} onBack={() => undefined} intervalMs={100000} />);
    expect(screen.queryByTestId("day-map")).toBeNull();
  });
});
