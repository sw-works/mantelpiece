/* eslint-disable @typescript-eslint/no-explicit-any */
// Design preview on the Virtual Device: every redesigned screen with scripted data and
// public sample photos, cycling every few seconds so a screenshot loop can capture each.
// Off the normal build like LiveSpike: swap the entry in index.js by hand (see README).
import { useEffect, useState } from "react";
import { Ask, JoinTrip, MapView, People, ReviewChanges, Slideshow } from "@mantelpiece/tv-ui";
import { Highlights } from "@mantelpiece/tv-ui/src/screens/Highlights";
import { Invite } from "@mantelpiece/tv-ui/src/screens/Invite";
import { MemoriesHome } from "@mantelpiece/tv-ui/src/screens/MemoriesHome";
import { TripView } from "@mantelpiece/tv-ui/src/screens/TripView";
import { Stage } from "@mantelpiece/tv-ui/src/components/Stage";
import { Ticker } from "@mantelpiece/tv-ui/src/components/Ticker";
import { FakeApi } from "@mantelpiece/tv-ui/test/fakeApi";

const api = new FakeApi();
(api as any).mediaUrl = async (_t: string, id: string) => ({ url: `https://picsum.photos/seed/${id}/960/540`, expires_in: 3600 });
(api as any).tripMap = async () => ({ url: "https://picsum.photos/seed/map/880/496?grayscale", inferred: false });
const day = (index: number, date: string, title: string, media: string[], story: string) => ({ index, date, title, media, route: { points: [], inferred: false, map_key: "k" }, story });
const kyoto = {
  id: "kyoto", name: "Kyoto in autumn", start: "2025-11-12T00:00:00Z", end: "2025-11-16T00:00:00Z", hero: "k1", highlights: ["k1", "k2", "k3", "k4", "k5", "k6", "k7", "k8"], contributors: ["maya", "tom", "hanyu"],
  days: [
    day(1, "2025-11-12", "Arrival", ["k1", "k2", "k3"], "A rainy arrival at Kyoto Station and the first bowl of ramen."),
    day(2, "2025-11-13", "Fushimi Inari", ["k4", "k5", "k6", "k7", "k8", "k9"], "Up before the crowds, the three of you walked the torii gates from the shrine to the Yotsutsuji lookout. Maya's shots of the fox statues anchor the morning; Tom caught the city opening out below at 09:40."),
    day(3, "2025-11-14", "Arashiyama", ["k10", "k11", "k12"], "Bamboo, then the river."),
    day(4, "2025-11-15", "Gion at night", ["k13", "k14"], "Lanterns."),
    day(5, "2025-11-16", "Nara day trip", ["k15", "k16", "k17"], "Deer."),
  ],
  narrative: { text: "Five days between temples and ramen counters.", model_id: "m", generated_at: "" },
  route: { points: [], inferred: false, map_key: "k" },
  contributor_names: { maya: "Maya", tom: "Tom", hanyu: "Hanyu" },
  contributor_roles: { maya: "co_editor", tom: "contributor", hanyu: "owner" } as any,
  media_credits: { k1: "hanyu", k2: "maya", k3: "tom", k4: "maya", k5: "tom", k6: "hanyu", k7: "maya", k8: "tom", k9: "maya" },
};
api.tripDetail = kyoto as any;
api.tripList = [
  kyoto,
  { id: "lisbon", name: "Lisbon with the Ferreiras", start: "2025-06-02T00:00:00Z", end: "2025-06-09T00:00:00Z", hero: "l1", highlights: [], contributors: ["a", "b"], shared_by: "the Ferreiras" },
  { id: "tahoe", name: "Lake Tahoe weekend", start: "2025-08-15T00:00:00Z", end: "2025-08-17T00:00:00Z", hero: "t1", highlights: [], contributors: ["a"] },
  { id: "tokyo", name: "Tokyo layover", start: "2025-11-01T00:00:00Z", end: "2025-11-01T00:00:00Z", hero: "y1", highlights: [], contributors: ["a"] },
  { id: "zermatt", name: "Zermatt", start: "2024-02-10T00:00:00Z", end: "2024-02-14T00:00:00Z", hero: "z1", highlights: [], contributors: ["a", "b"] },
  { id: "porto", name: "Porto", start: "2023-09-16T00:00:00Z", end: "2023-09-20T00:00:00Z", hero: "p1", highlights: [], contributors: ["a"] },
] as any;
api.memories = { date: "2023-09-16", on_this_day: ["p1", "p2", "p3", "p4", "p5"], then_and_now: [{ then: "z1", now: "z2", lat: 46, lon: 7.7, years_apart: 9 }], best_of_month: { year: 2025, month: "August", media: [] }, best_of_year: { year: 2025, media: [] } };
api.reactionsView = { reactions: [], summary: { "media:k4": { likes: 24, comments: 0, latest_comment: null }, "media:k6": { likes: 9, comments: 1, latest_comment: { by: "Tom", text: "Still can't believe we beat the crowds." } }, "day:2": { likes: 3, comments: 1, latest_comment: { by: "Tom", text: "Best morning of the trip." } } } };
api.peopleState = { people_enabled: true, people: [{ id: "p1", name: "Maya", media_count: 412, sample_media: ["k2", "k4", "k7", "k9", "k11"], faces: 40 }, { id: "p2", name: "Tom", media_count: 388, sample_media: ["k3", "k5"], faces: 30 }, { id: "p3", name: "Nora", media_count: 96, sample_media: ["k10"], faces: 9 }, { id: "p4", name: null, media_count: 61, sample_media: ["k12"], faces: 6 }] };
api.mapData = { url: "https://picsum.photos/seed/world/1080/870?grayscale", pins: [{ trip: "kyoto", name: "Kyoto in autumn", lat: 35, lon: 135, start: "Nov 2025" }, { trip: "tokyo", name: "Tokyo layover", lat: 35.6, lon: 139, start: "Nov 2025" }, { trip: "tahoe", name: "Lake Tahoe weekend", lat: 39, lon: -120, start: "Aug 2025" }, { trip: "lisbon", name: "Lisbon with the Ferreiras", lat: 38.7, lon: -9, start: "Jun 2025" }] };
api.searchResults = ["k1", "k2", "k3", "k4", "k5", "k6", "k7", "k8"].map((media) => ({ media, score: 1, trip: "kyoto", contributor: "maya" }));
api.proposalList = [
  { id: "p1", kind: "dedup", action: { type: "apply_dedup" }, trip: "kyoto", before: 214, after: 185, state: "open", requested_by: "concierge", expires_at: null, samples: [{ keep: "k4", remove: ["k5", "k6", "k7", "k8"], reason: "same second, sharper" }, { keep: "k9", remove: ["k10", "k11", "k12"], reason: "same frame" }, { keep: "k1", remove: ["k2", "k3"], reason: "exposure" }, { keep: "k13", remove: ["k14"], reason: "burst" }, { keep: "k15", remove: ["k16"], reason: "burst" }] },
  { id: "p2", kind: "cleanup", action: { type: "apply_cleanup" }, trip: null, before: 1462, after: 1450, state: "open", requested_by: "concierge", expires_at: null, samples: [{ keep: null, remove: ["s1", "s2", "s3"], reason: "screenshot text" }, { keep: null, remove: ["s4", "s5"], reason: "receipt" }] },
] as any;
(api as any).householdCode = async () => ({ code: "ABC234", scope: "trip", trip: "kyoto", link: "https://upload.mantelpiece.test/?t=ABC234", accepts_uploads: true, contributors: 3, member_households: 2 });
(api as any).tripCode = (api as any).householdCode;

const nav = { tvName: "Living room TV", items: ["Memories", "Map", "People", "Ask", "Join a trip", "Add photos", "Review"].map((label, i) => ({ key: ["home", "map", "people", "ask", "join", "invite", "review"][i]!, label, onPress: () => undefined, badge: label === "Review" ? 2 : undefined })) };
const noop = () => undefined;
const T = "t";
const SCREENS: Array<[string, () => any]> = [
  ["join", () => <JoinTrip api={api} token={T} onJoined={noop} onBack={noop} nav={nav} />],
  ["trip", () => <TripView api={api} token={T} tripId="kyoto" onBack={noop} onInvite={noop} onHighlights={noop} onPlay={noop} nav={nav} />],
  ["home", () => <><MemoriesHome api={api} token={T} tripsVersion={0} onOpenTrip={noop} onInvite={noop} onAsk={noop} onReview={noop} pendingReviews={2} onJoin={noop} onMap={noop} onPlay={noop} onPeople={noop} tvName="Living room TV" /><Ticker line="Maya added 42 photos from day 3" /></>],
  ["arriving", () => <MemoriesHome api={api} token={T} tripsVersion={0} onOpenTrip={noop} onInvite={noop} onAsk={noop} onReview={noop} pendingReviews={2} onJoin={noop} onMap={noop} onPlay={noop} onPeople={noop} tvName="Living room TV" arriving={{ done: 38, total: 120, from: "Maya" }} />],
  ["trip", () => <TripView api={api} token={T} tripId="kyoto" onBack={noop} onInvite={noop} onHighlights={noop} onPlay={noop} nav={nav} />],
  ["slideshow", () => <Slideshow api={api} token={T} trip={kyoto as any} startAt="k4" onBack={noop} intervalMs={100000} narration answer={{ question: "What's that gate?", answer: "The Senbon Torii at Fushimi Inari Taisha. About ten thousand gates, each donated by a business." }} />],
  ["invite", () => <Invite api={api} token={T} tripId="kyoto" tripName="Kyoto in autumn" ticker="Maya added 42 photos from day 3" onBack={noop} />],
  ["ask", () => <Ask api={api} token={T} onPlay={noop} onBack={noop} nav={nav} />],
  ["people", () => <People api={api} token={T} onPlay={noop} onBack={noop} nav={nav} />],
  ["review", () => <ReviewChanges api={api} token={T} onDone={noop} nav={nav} />],
  ["highlights", () => <Highlights api={api} token={T} tripId="kyoto" onBack={noop} onPlay={noop} nav={nav} />],
  ["join", () => <JoinTrip api={api} token={T} onJoined={noop} onBack={noop} nav={nav} />],
  ["map", () => <MapView api={api} token={T} onOpenTrip={noop} onBack={noop} nav={nav} />],
];
export const STEP_MS = 9000;

export function Preview() {
  const [i, setI] = useState(0);
  useEffect(() => {
    const t = setInterval(() => setI((n) => (n + 1) % SCREENS.length), STEP_MS);
    return () => clearInterval(t);
  }, []);
  const [name, render] = SCREENS[i]!;
  console.log(`preview: ${name}`);
  return <Stage key={name}>{render()}</Stage>;
}
