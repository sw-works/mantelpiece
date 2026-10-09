import { fireEvent, render, screen, waitFor } from "@testing-library/react-native";
import { containedRect, mapRect } from "../src/components/FieldNoteImage";
import { Slideshow } from "../src/screens/Slideshow";
import { ThisPlace } from "../src/screens/ThisPlace";
import { FakeApi } from "./fakeApi";

const route = { points: [], inferred: false, map_key: null };
const kyoto = {
  id: "kyoto",
  name: "Kyoto, April 2026",
  start: "2026-04-03T00:00:00Z",
  end: "2026-04-09T00:00:00Z",
  hero: "m1",
  highlights: ["m1"],
  contributors: ["owner"],
  days: [{ index: 1, date: "2026-04-03", media: ["m1", "m2"], title: "Day 1", route }],
  narrative: null,
  route,
  field_notes: { m1: { placement: { left: 0.5, top: 0.05, width: 0.4, height: 0.35 }, mode: "town" as const, certainty: "precise" as const, credit: "© OpenStreetMap contributors" } },
};

describe("field notes on the TV", () => {
  it("lines the map's focus box up with the whole photo on screen", () => {
    // A 3:2 photo in a 16:9 box is pillarboxed: 1620 wide, 150 px each side.
    expect(containedRect(1.5, 1920, 1080)).toEqual({ left: 150, top: 0, width: 1620, height: 1080 });
    // A portrait photo: 720 wide, centred.
    expect(containedRect(2 / 3, 1920, 1080)).toEqual({ left: 600, top: 0, width: 720, height: 1080 });
    const r = mapRect(kyoto.field_notes.m1, 1.5, 1920, 1080);
    expect(r.left).toBeCloseTo(150 + 0.5 * 1620);
    expect(r.top).toBeCloseTo(0.05 * 1080);
    expect(r.width).toBeCloseTo(0.4 * 1620);
  });

  it("shows the photo whole with its map, focuses the map from the remote, and opens This place", async () => {
    const api = new FakeApi();
    const onOpen = jest.fn();
    const onToggle = jest.fn();
    await render(<Slideshow api={api} token="t" trip={kyoto} onBack={() => undefined} intervalMs={100000} fieldNotes onFieldNotes={onToggle} onOpenPlace={onOpen} />);
    expect(screen.getByTestId("field-note")).toBeTruthy();
    expect(screen.queryByTestId("ken-burns")).toBeNull();
    // The photo reports its size; the map becomes a focus target over the note's box.
    const photo = await screen.findByTestId("image-m1-tv");
    await fireEvent(photo, "load", { nativeEvent: { source: { width: 1500, height: 1000 } } });
    expect(await screen.findByTestId("image-m1-fieldnote")).toBeTruthy();
    const map = await screen.findByTestId("field-note-map");
    await fireEvent(map, "focus");
    expect(screen.getByTestId("field-note-hint")).toBeTruthy();
    await fireEvent.press(map);
    expect(onOpen).toHaveBeenCalledWith("m1");
    await fireEvent.press(screen.getByTestId("field-notes"));
    expect(onToggle).toHaveBeenCalledWith(false);
  });

  it("plays plain photos with Ken Burns when Field notes is off, and says so when a trip has none", async () => {
    const api = new FakeApi();
    const view = await render(<Slideshow api={api} token="t" trip={kyoto} onBack={() => undefined} intervalMs={100000} onFieldNotes={() => undefined} />);
    expect(screen.getByTestId("ken-burns")).toBeTruthy();
    expect(screen.queryByTestId("field-note")).toBeNull();
    expect(screen.getByText("Field notes")).toBeTruthy();
    await view.rerender(<Slideshow api={api} token="t" trip={{ ...kyoto, field_notes: {} }} onBack={() => undefined} intervalMs={100000} fieldNotes onFieldNotes={() => undefined} />);
    expect(screen.getByTestId("no-field-notes")).toBeTruthy();
  });

  it("This place says when, where and what, with the credit, and Back returns", async () => {
    const api = new FakeApi();
    api.placeResult = {
      media: "m1",
      taken: "Saturday 4 April 2026, 14:00",
      place: "Kiyomizu-dera, Japan",
      coords: "34.99° N, 135.79° E",
      feature: { name: "Kiyomizu-dera", kind: "landmark", detail: null, summary: "Kiyomizu-dera is a Buddhist temple in eastern Kyoto.", tagline: "Buddhist temple", credit: "Wikipedia, CC BY-SA 4.0", url: null },
      description: "A wooden stage over the trees.",
      nearby: ["m2"],
      credit: "© OpenStreetMap contributors",
    };
    const onBack = jest.fn();
    await render(<ThisPlace api={api} token="t" tripId="kyoto" mediaId="m1" onBack={onBack} />);
    await waitFor(() => expect(screen.getByTestId("this-place")).toBeTruthy());
    expect(screen.getByTestId("this-place-name").props.children).toBe("Kiyomizu-dera, Japan");
    expect(screen.getByTestId("this-place-taken").props.children).toBe("Saturday 4 April 2026, 14:00");
    expect(screen.getByText("Kiyomizu-dera is a Buddhist temple in eastern Kyoto.")).toBeTruthy();
    expect(screen.getByText("From Wikipedia, CC BY-SA 4.0")).toBeTruthy();
    expect(screen.getByTestId("this-place-description")).toBeTruthy();
    expect(api.calls).toContain("place kyoto m1");
    await fireEvent.press(screen.getByTestId("back"));
    expect(onBack).toHaveBeenCalled();
  });
});
