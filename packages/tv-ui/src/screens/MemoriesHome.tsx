import { useEffect, useRef, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { type MemoriesHomeView, playlistAsTrip, type Trip, type TripSummary, type TvApi, longDate } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { PhotoCard } from "../components/PhotoCard";
import { type RailItem } from "../components/Rail";
import { RemoteImage } from "../components/RemoteImage";
import { type Nav, Screen } from "../components/Screen";
import { Eyebrow, Heading, Meta, Title } from "../components/Type";
import { theme } from "../theme";
import { groupByYear } from "../trips";

/** Uploads on the way in from a phone, off the feed's `upload.progress` (F-102). */
export interface Arriving {
  done: number;
  total: number;
  /** Who is sending, when the feed said. */
  from?: string | null;
}

const CARD_W = 330;
const CARD_H = 200;

/**
 * F-402: the newest memory is the screen. Its photo fills the top, its name and actions
 * sit over it; below, On this day, Then and now and the other trips as card rows under a
 * heading per year. While photos arrive or a trip is being organised the hero says so.
 */
export function MemoriesHome({
  api,
  token,
  tripsVersion,
  onOpenTrip,
  onInvite,
  onAsk,
  onReview,
  pendingReviews = 0,
  waitingPhones = 0,
  onJoin,
  onLeave,
  onMap,
  onPlay,
  onPeople,
  arriving = null,
  tvName,
}: {
  api: TvApi;
  token: string;
  tripsVersion: number;
  onOpenTrip: (trip: TripSummary) => void;
  onInvite: () => void;
  onAsk?: () => void;
  onReview?: () => void;
  pendingReviews?: number;
  /** Phones waiting for Allow; shown as the badge on Add photos, which opens pairing. */
  waitingPhones?: number;
  onJoin?: () => void;
  onLeave?: (trip: TripSummary) => Promise<void> | void;
  onMap?: () => void;
  onPlay?: (trip: Trip) => void;
  onPeople?: () => void;
  arriving?: Arriving | null;
  tvName?: string;
}) {
  const [trips, setTrips] = useState<TripSummary[] | null>(null);
  const [memories, setMemories] = useState<MemoriesHomeView | null>(null);
  useEffect(() => {
    let alive = true;
    api
      .memoriesHome(token)
      .then((m) => alive && setMemories(m))
      .catch(() => alive && setMemories(null));
    return () => {
      alive = false;
    };
  }, [api, token, tripsVersion]);
  const [seen, setSeen] = useState<Set<string>>(new Set());
  const scroller = useRef<ScrollView>(null);
  const yearTop = useRef<Record<string, number>>({});
  useEffect(() => {
    let alive = true;
    api
      .trips(token)
      .then((t) => alive && setTrips(t))
      .catch(() => alive && setTrips([]));
    return () => {
      alive = false;
    };
  }, [api, token, tripsVersion]);

  const years = groupByYear(trips ?? []);
  const hero = years[0]?.trips[0] ?? null;
  const open = (trip: TripSummary) => {
    setSeen(new Set([...seen, trip.id]));
    onOpenTrip(trip);
  };
  const leave = (trip: TripSummary) => {
    if (!onLeave) return;
    void Promise.resolve(onLeave(trip)).then(() => setTrips((t) => (t ?? []).filter((x) => x.id !== trip.id)));
  };

  const items: RailItem[] = [
    { key: "home", label: "Memories", onPress: () => scroller.current?.scrollTo?.({ y: 0, animated: true }), testID: "home" },
    ...(onMap ? [{ key: "map", label: "Map", onPress: onMap, testID: "map" }] : []),
    ...(onPeople ? [{ key: "people", label: "People", onPress: onPeople, testID: "people" }] : []),
    ...(onAsk ? [{ key: "ask", label: "Ask", onPress: onAsk, testID: "ask" }] : []),
    ...(onJoin ? [{ key: "join", label: "Join a trip", onPress: onJoin, testID: "join" }] : []),
    { key: "invite", label: "Add photos", onPress: onInvite, testID: "invite", badge: waitingPhones },
    ...(onReview ? [{ key: "review", label: "Review", onPress: onReview, testID: "review", badge: pendingReviews }] : []),
  ];
  const nav: Nav = { items, tvName };

  return (
    <Screen nav={nav} active="home" contentStyle={{ marginRight: 0, paddingTop: 96 }}>
      <ScrollView ref={scroller} contentContainerStyle={{ gap: 40, paddingBottom: theme.safe.y }}>
        {trips === null && <Meta size={theme.body}>Loading…</Meta>}
        {trips !== null && trips.length === 0 && !arriving && (
          <View style={{ gap: 18, maxWidth: 1000 }}>
            <Eyebrow>Nothing here yet</Eyebrow>
            <Title size={theme.display}>Your first memory starts on a phone</Title>
            <Meta testID="empty" size={theme.body}>
              Press Add photos, scan the code with your phone, and pick a trip's worth of pictures. The trip takes shape on its own.
            </Meta>
            <FocusButton label="Add photos" onPress={onInvite} preferred />
          </View>
        )}
        {arriving ? <ArrivingHero arriving={arriving} onInvite={onInvite} /> : hero ? <TripHero api={api} token={token} trip={hero} isNew={!seen.has(hero.id)} onOpen={() => open(hero)} onPlay={onPlay} onLeave={onLeave ? () => leave(hero) : undefined} /> : null}

        {memories && memories.on_this_day.length > 0 && (
          <View testID="on-this-day" style={{ gap: 14 }}>
            <View style={{ flexDirection: "row", gap: 18, alignItems: "center" }}>
              <Heading>On this day</Heading>
              <Meta>{memories.date}</Meta>
              {onPlay && <FocusButton variant="chip" label={`Play these ${memories.on_this_day.length}`} testID="play-on-this-day" onPress={() => onPlay(playlistAsTrip(`On this day, ${memories.date}`, memories.on_this_day))} />}
            </View>
            <ScrollView horizontal style={{ flexGrow: 0 }} contentContainerStyle={{ gap: 20, paddingVertical: 8 }}>
              {memories.on_this_day.map((m) => (
                <PhotoCard key={m} api={api} token={token} mediaId={m} width={300} height={170} />
              ))}
            </ScrollView>
          </View>
        )}
        {memories && memories.then_and_now[0] && (
          <View testID="then-and-now" style={{ flexDirection: "row", gap: 16, alignItems: "center", backgroundColor: theme.panel, padding: 16, borderRadius: theme.radius, alignSelf: "flex-start", marginRight: theme.safe.x }}>
            <RemoteImage api={api} token={token} mediaId={memories.then_and_now[0].then} rendition="thumb" width={280} height={190} />
            <RemoteImage api={api} token={token} mediaId={memories.then_and_now[0].now} rendition="thumb" width={280} height={190} />
            <View style={{ gap: 6, paddingHorizontal: 12 }}>
              <Heading>Then and now</Heading>
              <Meta testID="then-and-now-line">{`Same place, ${memories.then_and_now[0].years_apart} year${memories.then_and_now[0].years_apart === 1 ? "" : "s"} apart`}</Meta>
            </View>
          </View>
        )}

        {years.map((group, g) => {
          // The newest trip is the hero above, so the row under it holds the others and
          // says so: a count that includes the hero reads as a missing card.
          const heroHere = g === 0 && Boolean(hero) && !arriving;
          const rest = heroHere ? group.trips.slice(1) : group.trips;
          if (heroHere && rest.length === 0 && years.length === 1) return null;
          const count = heroHere ? `${rest.length} more trip${rest.length === 1 ? "" : "s"}` : `${group.trips.length} trip${group.trips.length === 1 ? "" : "s"}`;
          return (
            <View key={group.year} testID={`year-${group.year}`} onLayout={(e) => (yearTop.current[group.year] = e.nativeEvent.layout.y)} style={{ gap: 14 }}>
              <View style={{ flexDirection: "row", gap: 18, alignItems: "center" }}>
                {years.length > 1 ? <Heading testID={`year-heading-${group.year}`}>{group.year}</Heading> : <Heading>{heroHere ? "Earlier trips" : "Trips"}</Heading>}
                {(rest.length > 0 || !heroHere) && <Meta>{count}</Meta>}
                {g === 0 && years.length > 1 && (
                  <View testID="year-jump" style={{ flexDirection: "row", gap: 10, marginLeft: 18 }}>
                    {years.map((y) => (
                      <FocusButton key={y.year} variant="chip" label={y.year} selected={y.year === group.year} testID={`jump-${y.year}`} onPress={() => scroller.current?.scrollTo?.({ y: yearTop.current[y.year] ?? 0, animated: true })} />
                    ))}
                  </View>
                )}
              </View>
              {rest.length > 0 && (
                <ScrollView horizontal style={{ flexGrow: 0 }} contentContainerStyle={{ gap: 20, paddingVertical: 8, paddingRight: theme.safe.x }}>
                  {rest.map((trip) => (
                    <View key={trip.id} testID={`trip-${trip.id}`} style={{ gap: 8 }}>
                      <PhotoCard api={api} token={token} mediaId={trip.hero} width={CARD_W} height={CARD_H} title={trip.name} meta={tripMeta(trip)} badge={trip.shared_by ? "Shared" : !seen.has(trip.id) ? "New" : undefined} onPress={() => open(trip)} />
                      {!seen.has(trip.id) && !trip.shared_by && (
                        <Text testID={`new-${trip.id}`} style={{ color: theme.accent, fontSize: theme.credit }}>
                          New memory
                        </Text>
                      )}
                      {trip.shared_by && (
                        <View style={{ flexDirection: "row", gap: 12, alignItems: "center" }}>
                          <Text testID={`shared-${trip.id}`} style={{ color: theme.accent, fontSize: theme.credit }}>{`Shared by ${trip.shared_by}`}</Text>
                          {onLeave && <FocusButton variant="chip" label="Leave" testID={`leave-${trip.id}`} onPress={() => leave(trip)} />}
                        </View>
                      )}
                    </View>
                  ))}
                </ScrollView>
              )}
            </View>
          );
        })}
      </ScrollView>
    </Screen>
  );
}

function tripMeta(trip: TripSummary): string {
  const [from, to] = [longDate(trip.start), longDate(trip.end)];
  return `${from === to ? from : `${from} to ${to}`} · ${trip.contributors.length} contributor${trip.contributors.length === 1 ? "" : "s"}`;
}

const HERO_W = 1480;
const HERO_H = 470;

/**
 * The newest trip as one bounded card: its photo, its name and its own two buttons inside
 * the frame, so nothing on it reads as a control for the whole screen. Adding photos lives
 * in the rail.
 */
function TripHero({ api, token, trip, isNew, onOpen, onPlay, onLeave }: { api: TvApi; token: string; trip: TripSummary; isNew: boolean; onOpen: () => void; onPlay?: (trip: Trip) => void; onLeave?: () => void }) {
  const label = trip.shared_by ? `Shared by ${trip.shared_by}` : isNew ? "New memory" : "Latest trip";
  return (
    <View style={{ gap: 14 }}>
      <Heading>Latest trip</Heading>
      <View testID={`trip-${trip.id}`} style={{ width: HERO_W, height: HERO_H, borderRadius: 36, overflow: "hidden", backgroundColor: theme.panel, borderWidth: 2, borderColor: theme.panel2 }}>
        {trip.hero && (
          <View style={{ position: "absolute", top: 0, left: 0 }}>
            <RemoteImage api={api} token={token} mediaId={trip.hero} width={HERO_W} height={HERO_H} radius={0} />
          </View>
        )}
        <View style={{ position: "absolute", top: 0, left: 0, width: HERO_W, height: HERO_H, backgroundColor: "#0b0b0f99" }} />
        <View style={{ flex: 1, justifyContent: "flex-end", padding: 44, gap: 14 }}>
          <Text testID={trip.shared_by ? `shared-${trip.id}` : isNew ? `new-${trip.id}` : undefined} style={{ color: theme.accent, fontSize: theme.eyebrow, fontWeight: "700", letterSpacing: 2.5, textTransform: "uppercase" }}>
            {label}
          </Text>
          <Title size={80}>{trip.name}</Title>
          <Meta size={theme.body} color={theme.fg}>{tripMeta(trip)}</Meta>
          <View style={{ flexDirection: "row", gap: 14, marginTop: 8 }}>
            <FocusButton label="Open trip" testID={`open-${trip.id}`} preferred onPress={onOpen} />
            {onPlay && <FocusButton label="Play slideshow" variant="ghost" testID={`play-${trip.id}`} onPress={() => void api.trip(token, trip.id).then(onPlay).catch(() => undefined)} />}
            {trip.shared_by && onLeave && <FocusButton label="Leave trip" variant="ghost" testID={`leave-${trip.id}`} onPress={onLeave} />}
          </View>
        </View>
      </View>
    </View>
  );
}

/** Photos on their way in, then the trip being organised; the TV never looks idle. */
function ArrivingHero({ arriving, onInvite }: { arriving: Arriving; onInvite: () => void }) {
  const organising = arriving.done >= arriving.total;
  const pct = arriving.total > 0 ? Math.min(1, arriving.done / arriving.total) : 0;
  return (
    <View testID={organising ? "organising" : "arriving"} style={{ gap: 18, maxWidth: 1100, minHeight: 380, justifyContent: "flex-end" }}>
      <Eyebrow>{organising ? `All ${arriving.total} photos in · organising` : `Photos arriving${arriving.from ? ` · from ${arriving.from}'s phone` : ""}`}</Eyebrow>
      <Title size={84}>{organising ? "A new trip is taking shape" : `${arriving.done} of ${arriving.total} photos in`}</Title>
      <Meta size={theme.body}>{organising ? "Days, route and story are being worked out from the photos. Usually a few minutes; this screen changes when it is ready." : "The trip takes shape once the uploads settle. Nothing to do on the remote."}</Meta>
      {!organising && (
        <View style={{ width: 700, height: 8, borderRadius: 4, backgroundColor: "#ffffff22" }}>
          <View style={{ width: 700 * pct, height: 8, borderRadius: 4, backgroundColor: theme.accent }} />
        </View>
      )}
      <View style={{ flexDirection: "row", gap: 14, marginTop: 6 }}>
        <FocusButton label="Invite more people" preferred onPress={onInvite} />
      </View>
    </View>
  );
}
