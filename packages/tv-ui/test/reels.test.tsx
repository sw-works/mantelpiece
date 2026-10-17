import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react-native";
import { useEffect } from "react";
import { Image, StyleSheet, Text } from "react-native";
import { App } from "../src/App";
import { ApiError, type Reel as ReelPlan, type Trip } from "../src/api/client";
import { coverSize, cropRect, lerpWindow, rectTransform } from "../src/components/CropMotion";
import { type FilmPlayerProps } from "../src/film";
import { type MusicPlayer } from "../src/music";
import { Film } from "../src/screens/Film";
import { MemoriesHome } from "../src/screens/MemoriesHome";
import { Reel } from "../src/screens/Reel";
import { TripView } from "../src/screens/TripView";
import { MemorySessionStore } from "../src/session";
import { FakeApi } from "./fakeApi";

const credit = 'Music: "Carefree" by Kevin MacLeod, CC BY 3.0';

const plan: ReelPlan = {
  trip: "kyoto",
  day: 2,
  title: { text: "Kyoto", sub: "Day 2", start_ms: 0, end_ms: 3600 },
  track: { id: "carefree", title: "Carefree", artist: "Kevin MacLeod", licence: "CC BY 3.0", credit, url: "https://cdn.test/music/carefree.m4a", bpm: 95.7 },
  duration_ms: 12_000,
  fade_out_ms: 2000,
  slides: [
    { media: "m1", aspect: 4 / 3, start_ms: 0, end_ms: 4000, from: { cx: 0.5, cy: 0.5, z: 1 }, to: { cx: 0.52, cy: 0.46, z: 1.1 }, label: { text: "Kiyomizu-dera", start_ms: 600, end_ms: 3600 } },
    { media: "m2", aspect: 16 / 9, start_ms: 4000, end_ms: 8000, from: { cx: 0.4, cy: 0.5, z: 1.1 }, to: { cx: 0.6, cy: 0.5, z: 1.1 }, label: null },
    { media: "m3", aspect: 3 / 4, start_ms: 8000, end_ms: 10_000, from: { cx: 0.5, cy: 0.3, z: 1 }, to: { cx: 0.5, cy: 0.6, z: 1.05 }, label: null },
    { media: "m4", aspect: 1.5, start_ms: 10_000, end_ms: 12_000, from: { cx: 0.5, cy: 0.5, z: 1.05 }, to: { cx: 0.5, cy: 0.5, z: 1 }, label: null },
  ],
  credit: { text: credit, start_ms: 9000, end_ms: 12_000 },
};

/** A scripted music player: the test moves its position by hand. */
class FakeMusic implements MusicPlayer {
  log: string[] = [];
  pos: number | null = 0;
  loading: Promise<void> = Promise.resolve();
  load(url: string) {
    this.log.push(`load ${url}`);
    return this.loading;
  }
  play() {
    this.log.push("play");
  }
  pause() {
    this.log.push("pause");
  }
  positionMs() {
    return this.pos;
  }
  stop() {
    this.log.push("stop");
  }
}

/** The number an animated style value stands at now. */
const valueOf = (v: unknown) => Number(JSON.parse(JSON.stringify(v)));
const styleOf = (testID: string) => StyleSheet.flatten(screen.getByTestId(testID).props.style) as Record<string, unknown>;
const opacityOf = (testID: string) => valueOf(styleOf(testID).opacity);
const transformOf = (testID: string) => {
  const t = styleOf(testID).transform as Array<Record<string, unknown>>;
  return { translateX: valueOf(t[0]!.translateX), translateY: valueOf(t[1]!.translateY), scale: valueOf(t[2]!.scale) };
};
const showing = (layer: "reel-current" | "reel-previous") => {
  const image = within(screen.getByTestId(layer)).queryAllByTestId(/^reel-image-/)[0];
  return image ? String(image.props.testID).replace("reel-image-", "") : null;
};
const advance = async (ms: number) => {
  await act(async () => {
    jest.advanceTimersByTime(ms);
  });
};
/** Lets resolved promises (the API, the music load) land. */
const settle = () => advance(0);

describe("crop windows (contracts/api.md)", () => {
  it("covers the frame at z = 1 with the window centred", () => {
    expect(cropRect({ cx: 0.5, cy: 0.5, z: 1 }, 16 / 9)).toEqual({ left: 0, top: 0, width: 1920, height: 1080 });
    // 4:3 is taller than the frame: s0 = 1920 / w, so 1920 x 1440, centred vertically.
    expect(cropRect({ cx: 0.5, cy: 0.5, z: 1 }, 4 / 3)).toEqual({ left: 0, top: -180, width: 1920, height: 1440 });
    // A portrait picture covers the width: 1920 x 2560.
    const portrait = cropRect({ cx: 0.5, cy: 0.4, z: 1 }, 3 / 4);
    expect(portrait.width).toBeCloseTo(1920);
    expect(portrait.height).toBeCloseTo(2560);
    expect(portrait.top).toBeCloseTo(540 - 0.4 * 2560);
  });

  it("zooms over the cover fit and follows the window centre", () => {
    const r = cropRect({ cx: 0.52, cy: 0.46, z: 1.1 }, 4 / 3);
    expect(r.width).toBeCloseTo(2112);
    expect(r.height).toBeCloseTo(1584);
    expect(r.left).toBeCloseTo(960 - 0.52 * 2112);
    expect(r.top).toBeCloseTo(540 - 0.46 * 1584);
  });

  it("clamps so the picture always covers the frame", () => {
    // A window centred on the left edge would leave black on the left: clamped to 0.
    expect(cropRect({ cx: 0, cy: 0.5, z: 1.2 }, 16 / 9).left).toBe(0);
    // ...and on the right edge to F.w - w * s.
    expect(cropRect({ cx: 1, cy: 0.5, z: 1.2 }, 16 / 9).left).toBeCloseTo(1920 - 1920 * 1.2);
    expect(cropRect({ cx: 0.5, cy: 0, z: 1 }, 4 / 3).top).toBe(0);
    expect(cropRect({ cx: 0.5, cy: 1, z: 1 }, 4 / 3).top).toBeCloseTo(1080 - 1440);
    // A zoom under 1 (never planned) centres the picture instead of a reversed clamp.
    expect(cropRect({ cx: 0.2, cy: 0.5, z: 0.5 }, 16 / 9).left).toBeCloseTo(480);
  });

  it("interpolates the window linearly and clamps progress", () => {
    const from = { cx: 0.5, cy: 0.5, z: 1 };
    const to = { cx: 0.6, cy: 0.4, z: 1.2 };
    const mid = lerpWindow(from, to, 0.5);
    expect(mid.cx).toBeCloseTo(0.55);
    expect(mid.cy).toBeCloseTo(0.45);
    expect(mid.z).toBeCloseTo(1.1);
    expect(lerpWindow(from, to, -1)).toEqual(from);
    expect(lerpWindow(from, to, 2)).toEqual(to);
  });

  it("turns a rect into a centre-origin transform of the cover-sized picture", () => {
    const base = coverSize(16 / 9);
    const rect = cropRect({ cx: 0.6, cy: 0.5, z: 1.1 }, 16 / 9);
    const t = rectTransform(rect, base);
    expect(t.scale).toBeCloseTo(1.1);
    // The left edge after scaling about the centre lands on rect.left.
    expect(base.width / 2 - (base.width / 2) * t.scale + t.translateX).toBeCloseTo(rect.left);
    expect(base.height / 2 - (base.height / 2) * t.scale + t.translateY).toBeCloseTo(rect.top);
  });
});

describe("Reel (F-713)", () => {
  let prefetch: jest.SpyInstance;
  beforeEach(() => {
    jest.useFakeTimers();
    prefetch = jest.spyOn(Image, "prefetch").mockResolvedValue(true);
  });
  afterEach(() => {
    prefetch.mockRestore();
    jest.useRealTimers();
  });

  const reelApi = () => {
    const api = new FakeApi();
    api.reelResult = plan;
    return api;
  };

  it("cuts on the music's clock, crossfades, and pauses the picture and the music together", async () => {
    const api = reelApi();
    const music = new FakeMusic();
    await render(<Reel api={api} token="t" tripId="kyoto" day={2} music={music} onBack={jest.fn()} smoothMs={0} />);
    await settle();
    expect(api.calls).toContain("reel kyoto day 2");
    expect(music.log).toEqual([`load ${plan.track.url}`, "play"]);
    await advance(100);
    expect(showing("reel-current")).toBe("m1");

    music.pos = 3999;
    await advance(100);
    expect(showing("reel-current")).toBe("m1");
    // The bar line: the next slide, with the last one under it while it fades in.
    music.pos = 4000;
    await advance(100);
    expect(showing("reel-current")).toBe("m2");
    expect(showing("reel-previous")).toBe("m1");
    expect(opacityOf("reel-current")).toBe(0);
    music.pos = 4150;
    await advance(100);
    expect(opacityOf("reel-current")).toBeCloseTo(0.5);
    music.pos = 4400;
    await advance(100);
    expect(screen.queryByTestId("reel-previous")).toBeNull();

    // Pause holds both: the music is paused and the picture stays where it is.
    const held = transformOf("crop-current");
    await fireEvent.press(screen.getByTestId("reel-playpause"));
    expect(music.log.at(-1)).toBe("pause");
    expect(screen.getByTestId("reel-paused")).toBeTruthy();
    await advance(10_000);
    expect(showing("reel-current")).toBe("m2");
    expect(transformOf("crop-current")).toEqual(held);
    await fireEvent.press(screen.getByTestId("reel-playpause"));
    expect(music.log.at(-1)).toBe("play");
    music.pos = 8000;
    await advance(100);
    expect(showing("reel-current")).toBe("m3");
  });

  it("moves each picture with the clock by the contract's crop formula", async () => {
    await render(<Reel api={reelApi()} token="t" tripId="kyoto" onBack={jest.fn()} smoothMs={0} />);
    await settle();
    await advance(2000);
    // Halfway through m1: the window halfway from `from` to `to`.
    const slide = plan.slides[0]!;
    const want = rectTransform(cropRect(lerpWindow(slide.from, slide.to, 0.5), slide.aspect), coverSize(slide.aspect));
    const got = transformOf("crop-current");
    expect(got.scale).toBeCloseTo(want.scale);
    expect(got.translateX).toBeCloseTo(want.translateX);
    expect(got.translateY).toBeCloseTo(want.translateY);
    await advance(1000);
    expect(transformOf("crop-current").scale).toBeGreaterThan(want.scale);
  });

  it("plays silently on the wall clock without a music player, and Pause holds it", async () => {
    await render(<Reel api={reelApi()} token="t" tripId="kyoto" onBack={jest.fn()} smoothMs={0} />);
    await settle();
    await advance(3900);
    expect(showing("reel-current")).toBe("m1");
    await advance(100);
    expect(showing("reel-current")).toBe("m2");
    await fireEvent.press(screen.getByTestId("reel-playpause"));
    await advance(10_000);
    expect(showing("reel-current")).toBe("m2");
    await fireEvent.press(screen.getByTestId("reel-playpause"));
    await advance(4000);
    expect(showing("reel-current")).toBe("m3");
  });

  it("falls back to the wall clock when the music has no position, will not load, or stops moving", async () => {
    // No position: the wall clock.
    const silent = new FakeMusic();
    silent.pos = null;
    const first = await render(<Reel api={reelApi()} token="t" tripId="kyoto" music={silent} onBack={jest.fn()} smoothMs={0} />);
    await settle();
    await advance(4000);
    expect(showing("reel-current")).toBe("m2");
    await first.unmount();

    // A load that never finishes: the reel starts silently and the music is let go.
    const stuck = new FakeMusic();
    stuck.loading = new Promise(() => undefined);
    const second = await render(<Reel api={reelApi()} token="t" tripId="kyoto" music={stuck} onBack={jest.fn()} smoothMs={0} musicTimeoutMs={500} />);
    await settle();
    expect(screen.getByTestId("reel-loading")).toBeTruthy();
    await advance(500);
    expect(stuck.log).toEqual([`load ${plan.track.url}`, "stop"]);
    await advance(4000);
    expect(showing("reel-current")).toBe("m2");
    await second.unmount();

    // Music that stalls: held while it might recover, then the reel carries on silently.
    const stalled = new FakeMusic();
    stalled.pos = 1000;
    await render(<Reel api={reelApi()} token="t" tripId="kyoto" music={stalled} onBack={jest.fn()} smoothMs={0} stallMs={2000} />);
    await settle();
    await advance(2000);
    expect(showing("reel-current")).toBe("m1");
    await advance(3200);
    expect(showing("reel-current")).toBe("m2");
  });

  it("shows the title, each label and the credit in their windows, fading in and out", async () => {
    await render(<Reel api={reelApi()} token="t" tripId="kyoto" onBack={jest.fn()} smoothMs={0} />);
    await settle();
    await advance(100);
    expect(screen.getByTestId("reel-title-text").props.children).toBe("Kyoto");
    expect(screen.getByTestId("reel-title-sub").props.children).toBe("Day 2");
    expect(opacityOf("reel-title")).toBeCloseTo(1 / 3);
    expect(screen.queryByTestId("reel-label")).toBeNull();
    await advance(900);
    expect(opacityOf("reel-title")).toBe(1);
    expect(screen.getByTestId("reel-label-text").props.children).toBe("Kiyomizu-dera");
    expect(opacityOf("reel-label")).toBe(1);
    await advance(2500);
    // 3500: 100 ms before both end, two thirds of the way out.
    expect(opacityOf("reel-title")).toBeCloseTo(1 / 3);
    expect(opacityOf("reel-label")).toBeCloseTo(1 / 3);
    await advance(200);
    expect(screen.queryByTestId("reel-title")).toBeNull();
    expect(screen.queryByTestId("reel-label")).toBeNull();
    expect(screen.queryByTestId("reel-credit")).toBeNull();
    await advance(5600);
    // 9300: the credit is up.
    expect(screen.getByTestId("reel-credit-text").props.children).toBe(credit);
    expect(opacityOf("reel-credit")).toBe(1);
  });

  it("fetches each picture ahead of its cut", async () => {
    await render(<Reel api={reelApi()} token="t" tripId="kyoto" onBack={jest.fn()} smoothMs={0} />);
    await settle();
    const fetched = () => prefetch.mock.calls.map((c) => String(c[0]));
    expect(fetched()).toEqual(["https://cdn.test/m1.jpg", "https://cdn.test/m2.jpg", "https://cdn.test/m3.jpg"]);
    await advance(4000);
    expect(fetched()).toContain("https://cdn.test/m4.jpg");
    expect(fetched()).toHaveLength(4);
  });

  it("fades to black at the end, stops the music, and offers Play again", async () => {
    const music = new FakeMusic();
    await render(<Reel api={reelApi()} token="t" tripId="kyoto" music={music} onBack={jest.fn()} smoothMs={0} />);
    await settle();
    music.pos = 9000;
    await advance(100);
    expect(opacityOf("reel-fade")).toBe(0);
    music.pos = 11_000;
    await advance(100);
    expect(opacityOf("reel-fade")).toBeCloseTo(0.5);
    expect(music.log).not.toContain("stop");
    music.pos = 12_000;
    await advance(100);
    expect(screen.getByTestId("reel-ended")).toBeTruthy();
    expect(screen.getByTestId("reel-ended-credit").props.children).toBe(credit);
    expect(music.log.at(-1)).toBe("stop");
    music.pos = 0;
    await fireEvent.press(screen.getByTestId("play-again"));
    await settle();
    expect(music.log.slice(-2)).toEqual([`load ${plan.track.url}`, "play"]);
    await advance(100);
    expect(showing("reel-current")).toBe("m1");
  });

  it("stops the music on Back", async () => {
    const music = new FakeMusic();
    const onBack = jest.fn();
    await render(<Reel api={reelApi()} token="t" tripId="kyoto" music={music} onBack={onBack} smoothMs={0} />);
    await settle();
    await advance(100);
    await fireEvent.press(screen.getByTestId("back"));
    expect(music.log).toContain("stop");
    expect(onBack).toHaveBeenCalled();
  });

  it("says when a day has too few photos for a reel", async () => {
    const api = new FakeApi();
    api.reelResult = new ApiError(422, "too few photos");
    const music = new FakeMusic();
    const onBack = jest.fn();
    await render(<Reel api={api} token="t" tripId="kyoto" day={3} music={music} onBack={onBack} />);
    await settle();
    expect(screen.getByTestId("reel-too-few")).toBeTruthy();
    expect(music.log).toEqual([]);
    await fireEvent.press(screen.getByTestId("back"));
    expect(onBack).toHaveBeenCalled();
  });
});

describe("Film (F-714)", () => {
  const film = { day: 2, url: "https://cdn.test/films/day-2.mp4", duration_ms: 41_240, made_at: "2026-10-17T20:01:12Z" };

  function FakeFilmPlayer({ uri, onState }: FilmPlayerProps) {
    useEffect(() => onState?.("playing"), [onState]);
    return <Text testID="film-player">{uri}</Text>;
  }

  it("plays the day's film full screen with the platform player", async () => {
    const api = new FakeApi();
    api.films = { 2: film };
    const onBack = jest.fn();
    await render(<Film api={api} token="t" tripId="kyoto" tripName="Kyoto" day={2} player={FakeFilmPlayer} onBack={onBack} />);
    expect(await screen.findByTestId("film-player")).toBeTruthy();
    expect(screen.getByTestId("film-player").props.children).toBe(film.url);
    expect(api.calls).toContain("film kyoto day 2");
    expect(screen.queryByTestId("film-state")).toBeNull();
    await fireEvent.press(screen.getByTestId("back"));
    expect(onBack).toHaveBeenCalled();
  });

  it("offers Play again once the film has ended", async () => {
    const api = new FakeApi();
    api.films = { 2: film };
    function EndingPlayer({ onState }: FilmPlayerProps) {
      useEffect(() => onState?.("ended"), [onState]);
      return <Text testID="film-player">end</Text>;
    }
    await render(<Film api={api} token="t" tripId="kyoto" tripName="Kyoto" day={2} player={EndingPlayer} onBack={jest.fn()} />);
    expect(await screen.findByTestId("film-again")).toBeTruthy();
    expect(screen.getByTestId("film-state").props.children).toBe("Kyoto · Day 2 · The end");
  });

  it("says when this TV has no film player", async () => {
    const api = new FakeApi();
    api.films = { 2: film };
    await render(<Film api={api} token="t" tripId="kyoto" tripName="Kyoto" day={2} onBack={jest.fn()} />);
    expect(screen.getByTestId("no-film-player-line").props.children).toBe("This TV cannot play films yet.");
    expect(api.calls).not.toContain("film kyoto day 2");
  });

  it("says when the day has no film yet", async () => {
    await render(<Film api={new FakeApi()} token="t" tripId="kyoto" tripName="Kyoto" day={2} player={FakeFilmPlayer} onBack={jest.fn()} />);
    expect((await screen.findByTestId("no-film-line")).props.children).toBe("There is no film of day 2 yet.");
  });
});

describe("reel and film entry points", () => {
  const noRoute = { points: [], inferred: false, map_key: null };
  const kyoto: Trip = {
    id: "kyoto",
    name: "Kyoto, April 2026",
    start: "2026-04-03T00:00:00Z",
    end: "2026-04-09T00:00:00Z",
    hero: "m1",
    highlights: ["m1"],
    contributors: ["owner"],
    days: [
      { index: 1, date: "2026-04-03", media: ["m1", "m2", "m3"], title: "Day 1", route: noRoute },
      { index: 2, date: "2026-04-04", media: ["m4"], title: "Day 2", route: noRoute },
    ],
    narrative: null,
    route: noRoute,
  };
  const film = { day: 1, url: "https://cdn.test/films/day-1.mp4", duration_ms: 30_000, made_at: "2026-10-17T20:01:12Z" };

  it("TripView plays the trip's reel, a day's reel and its film, and makes a film on request", async () => {
    const api = new FakeApi();
    api.tripDetail = kyoto;
    api.films = { 1: film };
    const onReel = jest.fn();
    const onFilm = jest.fn();
    await render(<TripView api={api} token="t" tripId="kyoto" onBack={() => undefined} onInvite={() => undefined} onReel={onReel} onFilm={onFilm} />);
    await fireEvent.press(await screen.findByTestId("play-reel"));
    expect(onReel).toHaveBeenLastCalledWith(expect.objectContaining({ id: "kyoto" }));
    await fireEvent.press(screen.getByTestId("day-reel-1"));
    expect(onReel).toHaveBeenLastCalledWith(expect.objectContaining({ id: "kyoto" }), 1);
    await fireEvent.press(await screen.findByTestId("day-film-1"));
    expect(onFilm).toHaveBeenCalledWith(expect.objectContaining({ id: "kyoto" }), 1);
    expect(api.calls).toContain("film kyoto day 1");

    await fireEvent.press(screen.getByTestId("make-film-1"));
    await waitFor(() => expect(screen.getByTestId("film-queued").props.children).toBe("Your film will be ready in a few minutes"));
    expect(api.calls).toContain("make film kyoto day 1");
    expect(screen.queryByTestId("make-film-1")).toBeNull();

    // A day with fewer than three photos has neither a reel nor a film.
    await fireEvent.press(screen.getByTestId("day-2"));
    expect(screen.queryByTestId("day-actions")).toBeNull();
    expect(api.calls).not.toContain("film kyoto day 2");
  });

  it("TripView shows the film once the feed says it is ready, and says when making one failed", async () => {
    const api = new FakeApi();
    api.tripDetail = kyoto;
    api.makeFilmError = new ApiError(503, "films are off");
    const props = { api, token: "t", tripId: "kyoto", onBack: () => undefined, onInvite: () => undefined, onReel: jest.fn(), onFilm: jest.fn() };
    const view = await render(<TripView {...props} filmsVersion={0} />);
    await screen.findByTestId("day-reel-1");
    await waitFor(() => expect(api.calls).toContain("film kyoto day 1"));
    expect(screen.queryByTestId("day-film-1")).toBeNull();
    await fireEvent.press(screen.getByTestId("make-film-1"));
    await waitFor(() => expect(screen.getByTestId("film-failed")).toBeTruthy());

    api.films = { 1: film };
    await view.rerender(<TripView {...props} filmsVersion={1} />);
    expect(await screen.findByTestId("day-film-1")).toBeTruthy();
    expect(screen.queryByTestId("film-failed")).toBeNull();
  });

  it("TripView keeps film making to the owning household", async () => {
    const api = new FakeApi();
    api.tripDetail = { ...kyoto, shared_by: "Hanyu's living room" };
    api.films = { 1: film };
    await render(<TripView api={api} token="t" tripId="kyoto" onBack={() => undefined} onInvite={() => undefined} onReel={jest.fn()} onFilm={jest.fn()} canManage={false} />);
    expect(await screen.findByTestId("day-film-1")).toBeTruthy();
    expect(screen.getByTestId("day-reel-1")).toBeTruthy();
    expect(screen.queryByTestId("make-film-1")).toBeNull();
  });

  it("MemoriesHome plays the latest trip's reel", async () => {
    const api = new FakeApi();
    api.tripList = [{ id: "kyoto", name: "Kyoto, April 2026", start: kyoto.start, end: kyoto.end, hero: "m1", highlights: [], contributors: ["owner"] }];
    api.tripDetail = kyoto;
    const onReel = jest.fn();
    await render(<MemoriesHome api={api} token="t" tripsVersion={0} onOpenTrip={() => undefined} onInvite={() => undefined} onPlay={jest.fn()} onReel={onReel} />);
    expect(await screen.findByTestId("play-kyoto")).toBeTruthy();
    await fireEvent.press(screen.getByTestId("reel-kyoto"));
    await waitFor(() => expect(onReel).toHaveBeenCalledWith(expect.objectContaining({ id: "kyoto" })));
  });

  it("App opens a day's reel from the trip and Back returns to the trip", async () => {
    const api = new FakeApi();
    api.tripList = [{ id: "kyoto", name: "Kyoto, April 2026", start: kyoto.start, end: kyoto.end, hero: "m1", highlights: [], contributors: ["owner"] }];
    api.tripDetail = kyoto;
    api.reelResult = plan;
    const store = new MemorySessionStore();
    await store.save({ token: "tv-token", household: "tv-house", deviceId: "tv-1" });
    const music = new FakeMusic();
    await render(<App api={api} sessionStore={store} idleMs={0} music={music} />);
    await fireEvent.press(await screen.findByTestId("open-kyoto"));
    await fireEvent.press(await screen.findByTestId("day-reel-1"));
    expect(await screen.findByTestId("reel")).toBeTruthy();
    expect(api.calls).toContain("reel kyoto day 1");
    expect(music.log).toEqual([`load ${plan.track.url}`, "play"]);
    await fireEvent.press(screen.getByTestId("back"));
    expect(await screen.findByTestId("day-reel-1")).toBeTruthy();
    expect(music.log).toContain("stop");
  });

  it("App says the TV cannot play films without a film player", async () => {
    const api = new FakeApi();
    api.tripList = [{ id: "kyoto", name: "Kyoto, April 2026", start: kyoto.start, end: kyoto.end, hero: "m1", highlights: [], contributors: ["owner"] }];
    api.tripDetail = kyoto;
    api.films = { 1: film };
    const store = new MemorySessionStore();
    await store.save({ token: "tv-token", household: "tv-house", deviceId: "tv-1" });
    await render(<App api={api} sessionStore={store} idleMs={0} />);
    await fireEvent.press(await screen.findByTestId("open-kyoto"));
    await fireEvent.press(await screen.findByTestId("day-film-1"));
    expect((await screen.findByTestId("no-film-player-line")).props.children).toBe("This TV cannot play films yet.");
    await fireEvent.press(screen.getByTestId("back"));
    expect(await screen.findByTestId("day-film-1")).toBeTruthy();
  });
});
