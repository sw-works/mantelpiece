import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { View } from "react-native";
import { ApiError, playlistAsTrip, type Trip, TvApi } from "./api/client";
import { type Nav } from "./components/Screen";
import { Stage } from "./components/Stage";
import { Ticker } from "./components/Ticker";
import { useEvents } from "./hooks/useEvents";
import { useRemoteKeys } from "./hooks/useRemoteKeys";
import { type AudioPlayer } from "./audio";
import { type LivePlayer } from "./live";
import { Ambient } from "./screens/Ambient";
import { Arrived } from "./screens/Arrived";
import { Ask } from "./screens/Ask";
import { Highlights } from "./screens/Highlights";
import { Invite } from "./screens/Invite";
import { JoinTrip } from "./screens/JoinTrip";
import { Live } from "./screens/Live";
import { LiveRequestPrompt } from "./components/LiveRequestPrompt";
import { MapView } from "./screens/MapView";
import { MemoriesHome } from "./screens/MemoriesHome";
import { People } from "./screens/People";
import { Phones } from "./screens/Phones";
import { ReviewChanges } from "./screens/ReviewChanges";
import { ThisPlace } from "./screens/ThisPlace";
import { Slideshow } from "./screens/Slideshow";
import { TripView } from "./screens/TripView";
import { Welcome } from "./screens/Welcome";
import { MemorySessionStore, newDeviceId, type SessionStore } from "./session";

export type Route =
  | { name: "home" }
  | { name: "trip"; id: string; shared?: boolean }
  | { name: "join" }
  | { name: "map" }
  | { name: "people" }
  | { name: "ambient" }
  | { name: "highlights"; id: string }
  | { name: "slideshow"; trip: Trip; start?: string; resume?: boolean }
  /** A field note's map opened from the slideshow (F-711, US6); Back resumes the show. */
  | { name: "place"; trip: Trip; media: string }
  | { name: "ask" }
  | { name: "review" }
  | { name: "phones"; first?: boolean }
  | { name: "invite"; tripId: string | null }
  /** A phone's photos just landed; `back` is where the TV was (family channel). */
  | { name: "arrived"; back: Route }
  /** A phone is broadcasting; `back` is where the TV was (live from the event). */
  | { name: "live"; back: Route; from: string };

/** Shows made on the spot rather than a stored trip: no presence trip, no trip to go back to. */
function isAdHoc(tripId: string): boolean {
  return tripId.startsWith("playlist:") || tripId.startsWith("arrival:");
}

/** Replays after a cold start must not pop old photos or a finished broadcast up. */
const FRESH_ARRIVAL_MS = 10 * 60 * 1000;
const FRESH_LIVE_MS = 4 * 60 * 60 * 1000;
const fresh = (at: string, windowMs: number) => Date.now() - Date.parse(at) < windowMs;

export interface AppProps {
  apiBase?: string;
  api?: TvApi;
  sessionStore?: SessionStore;
  /** What this TV calls itself; the household name offered on first launch. */
  tvName?: string;
  /** Platform audio for narration; absent means captions only (F-709). */
  audio?: AudioPlayer | null;
  /** Read stories aloud or as captions in the slideshow. */
  narration?: boolean;
  /** Minutes of stillness on Memories home before the ambient slideshow (F-406). */
  idleMs?: number;
  /** Plays a phone's live broadcast; the platform shell supplies it. */
  livePlayer?: LivePlayer;
}

const DEFAULT_API = "https://api.mantelpiece.test";

/** State-based router: no navigation library, no native screens module. */
export function App({ apiBase, api: injectedApi, sessionStore, tvName, audio = null, narration = false, idleMs = 5 * 60 * 1000, livePlayer }: AppProps) {
  const api = useMemo(() => injectedApi ?? new TvApi(apiBase ?? DEFAULT_API), [injectedApi, apiBase]);
  const store = useMemo(() => sessionStore ?? new MemorySessionStore(), [sessionStore]);
  const [deviceId] = useState(() => newDeviceId());
  const [session, setSession] = useState<{ token: string; household: string; name?: string } | null | undefined>(undefined);
  // F-711: the Field notes toggle, remembered by this TV across trips and restarts.
  const [fieldNotes, setFieldNotes] = useState(false);
  const chooseFieldNotes = useCallback(
    (on: boolean) => {
      setFieldNotes(on);
      void store.load().then((s) => s && store.save({ ...s, fieldNotes: on }));
    },
    [store],
  );
  const [route, setRoute] = useState<Route>({ name: "home" });
  const feed = useEvents(api, session?.token ?? null);

  // Hardware Back mirrors the on-screen Back buttons. On the home screen it is left to the
  // platform, which exits the app.
  const goBack = useCallback(() => {
    setRoute((r) => {
      if (r.name === "trip") return { name: "home" };
      if (r.name === "highlights") return { name: "trip", id: r.id };
      if (r.name === "slideshow") return r.trip.id.startsWith("playlist:") ? { name: "ask" } : r.trip.id.startsWith("arrival:") ? { name: "home" } : { name: "trip", id: r.trip.id };
      if (r.name === "arrived" || r.name === "live") return r.back;
      if (r.name === "place") return { name: "slideshow", trip: r.trip, start: r.media, resume: true };
      if (r.name === "ask" || r.name === "review" || r.name === "phones" || r.name === "join" || r.name === "map" || r.name === "people" || r.name === "ambient")
        return { name: "home" };
      if (r.name === "invite") return r.tripId ? { name: "trip", id: r.tripId } : { name: "home" };
      return r;
    });
  }, []);
  // The slideshow owns the remote while it is up.
  useRemoteKeys({ onBack: route.name === "home" || route.name === "slideshow" || route.name === "place" || route.name === "ambient" || route.name === "arrived" || route.name === "live" ? undefined : goBack });

  useEffect(() => {
    store.load().then((s) => {
      setSession(s ? { token: s.token, household: s.household, ...(s.name ? { name: s.name } : {}) } : null);
      setFieldNotes(s?.fieldNotes === true);
    });
  }, [store]);

  // F-406: stillness on Memories home becomes the ambient slideshow.
  useEffect(() => {
    if (route.name !== "home" || !session || idleMs <= 0) return;
    const t = setTimeout(() => setRoute({ name: "ambient" }), idleMs);
    return () => clearTimeout(t);
  }, [route, session, idleMs]);

  // "Play Skye on the TV" from the Echo Show skill, the phone or Alexa+: the feed carries a
  // play.requested and the TV opens the slideshow, once per request (F-405 by voice).
  const playedSeq = useRef(0);
  useEffect(() => {
    const ask = feed.play;
    if (!ask || !session || ask.seq <= playedSeq.current) return;
    playedSeq.current = ask.seq;
    const load = ask.trip
      ? api.trip(session.token, ask.trip)
      : ask.playlist
        ? api.playlist(session.token, ask.playlist).then((p) => playlistAsTrip(p.name, p.media))
        : null;
    if (!load) return;
    let alive = true;
    load.then((trip) => alive && setRoute({ name: "slideshow", trip, ...(ask.start ? { start: ask.start } : {}) })).catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [feed.play, session, api]);

  // Family channel: a phone's photos take over the idle screens as they land, once each.
  const arrivedSeq = useRef(0);
  useEffect(() => {
    const a = feed.arrival;
    if (!a || !session || a.seq <= arrivedSeq.current) return;
    arrivedSeq.current = a.seq;
    if (!fresh(a.at, FRESH_ARRIVAL_MS)) return;
    setRoute((r) => (r.name === "ambient" || r.name === "home" ? { name: "arrived", back: r } : r));
  }, [feed.arrival, session]);

  // Live from the event: someone here let the broadcast in, so it takes the screen from
  // wherever the TV is; a switch to another phone stays on Live.
  const liveSeq = useRef(0);
  useEffect(() => {
    const l = feed.live;
    if (!l || !session || l.seq <= liveSeq.current) return;
    liveSeq.current = l.seq;
    if (!fresh(l.at, FRESH_LIVE_MS)) return;
    setRoute((r) => (r.name === "live" ? { ...r, from: l.from } : { name: "live", back: r.name === "arrived" ? r.back : r, from: l.from }));
  }, [feed.live, session]);

  // A phone asking to go live: one prompt at a time, oldest first, gone once answered here,
  // withdrawn, or out of time.
  const [answeredAsks, setAnsweredAsks] = useState<string[]>([]);
  const ask = (feed.liveRequests ?? []).find((a) => !answeredAsks.includes(a.id) && Date.parse(a.expiresAt) > Date.now()) ?? null;
  // The phone stopped asking, or the time ran out, while the TV answered: nothing to do.
  const gone = (p: Promise<unknown>) => p.catch((e: unknown) => {
    if (!(e instanceof ApiError && (e.status === 404 || e.status === 409))) throw e;
  });
  const dropAsk = useCallback((id: string) => setAnsweredAsks((done) => (done.includes(id) ? done : [...done, id])), []);

  // What is on screen, for "what's that statue" from the phone or Alexa+ (research R3).
  const token = session?.token;
  const onPresence = useCallback(
    (now: { media: string; day: number; paused: boolean }) => {
      if (!token) return;
      const trip = route.name === "slideshow" && !isAdHoc(route.trip.id) ? route.trip.id : null;
      api.putPresence(token, { device_id: deviceId, media: now.media, trip, day: now.day, paused: now.paused }).catch(() => undefined);
    },
    [api, token, deviceId, route],
  );
  // A phone asked to connect: bring up the approval queue, unless a show is playing. The
  // queue is fetched first because the feed replays old requests after a cold start.
  const phonesVersion = feed.phonesVersion ?? 0;
  const [waitingPhones, setWaitingPhones] = useState(0);
  useEffect(() => {
    if (!token) return;
    let alive = true;
    api
      .pairQueue(token)
      .then((q) => {
        if (!alive) return;
        setWaitingPhones(q.pending.length);
        if (q.pending.length > 0 && phonesVersion > 0) setRoute((r) => (r.name === "slideshow" || r.name === "ambient" || r.name === "phones" ? r : { name: "phones" }));
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [api, token, phonesVersion, route.name === "phones"]);
  const householdName = session?.name ?? tvName;
  const pendingReviews = feed.recent.filter((e) => e.type === "dedup.proposal" || e.type === "cleanup.proposal").length;
  const reactionsVersion = feed.recent.filter((e) => e.type === "reaction.received").length;
  const lastAnswer = [...feed.recent].reverse().find((e) => e.type === "answer.card");
  const answer = lastAnswer ? { question: String(lastAnswer.question), answer: String(lastAnswer.answer), media: lastAnswer.media as string | null } : null;
  // The rail every screen shares: the household's places, with the review count.
  const liveNow = feed.live ?? null;
  const nav: Nav = useMemo(
    () => ({
      tvName: householdName,
      items: [
        ...(liveNow ? [{ key: "live", label: `Live · ${liveNow.from}`, onPress: () => setRoute({ name: "live", back: { name: "home" }, from: liveNow.from }), testID: "live-now" }] : []),
        { key: "home", label: "Memories", onPress: () => setRoute({ name: "home" }), testID: "home" },
        { key: "map", label: "Map", onPress: () => setRoute({ name: "map" }), testID: "map" },
        { key: "people", label: "People", onPress: () => setRoute({ name: "people" }), testID: "people" },
        { key: "ask", label: "Ask", onPress: () => setRoute({ name: "ask" }), testID: "ask" },
        { key: "join", label: "Join a trip", onPress: () => setRoute({ name: "join" }), testID: "join" },
        // Adding photos starts with a paired phone; the badge counts phones waiting for Allow.
        { key: "invite", label: "Add photos", onPress: () => setRoute({ name: "phones" }), testID: "invite", badge: waitingPhones },
        { key: "review", label: "Review", onPress: () => setRoute({ name: "review" }), testID: "review", badge: pendingReviews },
      ],
    }),
    [householdName, pendingReviews, waitingPhones, liveNow],
  );

  if (session === undefined) return <View style={{ flex: 1, backgroundColor: "#0b0b0f" }} />;
  if (!session) {
    return (
      <Stage>
        <Welcome
          api={api}
          deviceId={deviceId}
          defaultName={tvName ?? "Fire TV"}
          onReady={(s) => {
            void store.save({ ...s, deviceId });
            setSession(s);
            setRoute({ name: "phones", first: true });
          }}
        />
      </Stage>
    );
  }

  let screen;
  switch (route.name) {
    case "home":
      screen = (
        <MemoriesHome
          api={api}
          token={session.token}
          tripsVersion={feed.tripsVersion}
          onOpenTrip={(t) => setRoute({ name: "trip", id: t.id, shared: Boolean(t.shared_by) })}
          onJoin={() => setRoute({ name: "join" })}
          onMap={() => setRoute({ name: "map" })}
          onPeople={() => setRoute({ name: "people" })}
          onPlay={(trip) => setRoute({ name: "slideshow", trip })}
          onLeave={(t) => api.leaveTrip(session.token, t.id).catch(() => undefined)}
          onInvite={() => setRoute({ name: "phones" })}
          onAsk={() => setRoute({ name: "ask" })}
          onReview={() => setRoute({ name: "review" })}
          pendingReviews={pendingReviews}
          waitingPhones={waitingPhones}
          arriving={feed.upload ?? null}
          tvName={householdName}
        />
      );
      break;
    case "phones":
      screen = (
        <Phones
          api={api}
          token={session.token}
          deviceId={deviceId}
          householdName={householdName ?? "This TV"}
          version={phonesVersion}
          first={route.first}
          onDone={() => setRoute({ name: "home" })}
        />
      );
      break;
    case "trip":
      screen = (
        <TripView
          api={api}
          token={session.token}
          tripId={route.id}
          onBack={() => setRoute({ name: "home" })}
          onInvite={(tripId) => setRoute({ name: "invite", tripId })}
          onHighlights={(id) => setRoute({ name: "highlights", id })}
          onPlay={(trip) => setRoute({ name: "slideshow", trip })}
          canManage={!route.shared}
          reactionsVersion={reactionsVersion}
          nav={nav}
        />
      );
      break;
    case "people":
      screen = <People api={api} token={session.token} onPlay={(trip) => setRoute({ name: "slideshow", trip })} onBack={goBack} nav={nav} />;
      break;
    case "map":
      screen = <MapView api={api} token={session.token} onOpenTrip={(id) => setRoute({ name: "trip", id })} onBack={goBack} nav={nav} />;
      break;
    case "join":
      screen = <JoinTrip api={api} token={session.token} onJoined={(id) => setRoute({ name: "trip", id, shared: true })} onBack={goBack} nav={nav} />;
      break;
    case "slideshow":
      screen = (
        <Slideshow
          api={api}
          token={session.token}
          trip={route.trip}
          startAt={route.start}
          onBack={goBack}
          onPresence={onPresence}
          answer={answer}
          audio={audio}
          narration={narration}
          fieldNotes={fieldNotes}
          onFieldNotes={chooseFieldNotes}
          onOpenPlace={isAdHoc(route.trip.id) ? undefined : (media) => setRoute({ name: "place", trip: route.trip, media })}
          startPaused={route.resume === true}
          focusMap={route.resume === true}
        />
      );
      break;
    case "place":
      screen = <ThisPlace api={api} token={session.token} tripId={route.trip.id} mediaId={route.media} onBack={goBack} />;
      break;
    case "ambient":
      screen = <Ambient api={api} token={session.token} onBack={goBack} onPresence={onPresence} />;
      break;
    case "ask":
      screen = <Ask api={api} token={session.token} onPlay={(trip) => setRoute({ name: "slideshow", trip })} onBack={goBack} nav={nav} />;
      break;
    case "review":
      screen = <ReviewChanges api={api} token={session.token} onDone={goBack} nav={nav} />;
      break;
    case "highlights":
      screen = (
        <Highlights
          api={api}
          token={session.token}
          tripId={route.id}
          onPlayFieldNotes={(trip) => {
            chooseFieldNotes(true);
            setRoute({ name: "slideshow", trip });
          }}
          onBack={() => setRoute({ name: "trip", id: route.id })}
          onPlay={(trip) => setRoute({ name: "slideshow", trip })}
          nav={nav}
        />
      );
      break;
    case "arrived":
      screen = feed.arrival ? (
        <Arrived
          api={api}
          token={session.token}
          arrival={feed.arrival}
          onPlayAll={(media, from) => setRoute({ name: "slideshow", trip: { ...playlistAsTrip(`From ${from}`, media), id: `arrival:${feed.arrival?.batch ?? ""}` } })}
          onClose={goBack}
        />
      ) : null;
      break;
    case "live":
      screen = (
        <Live
          live={feed.live ?? null}
          from={feed.live?.from ?? route.from}
          player={livePlayer}
          onBack={goBack}
          onEnd={() => void api.stopLive(session.token).catch(() => undefined)}
        />
      );
      break;
    case "invite":
      screen = (
        <Invite
          api={api}
          token={session.token}
          tripId={route.tripId}
          ticker={feed.ticker}
          onBack={() => setRoute(route.tripId ? { name: "trip", id: route.tripId } : { name: "home" })}
        />
      );
      break;
  }
  return (
    <Stage>
      {screen}
      {route.name === "home" && <Ticker line={feed.ticker} />}
      {ask && (
        <LiveRequestPrompt
          key={ask.id}
          ask={ask}
          onGrant={async () => {
            await gone(api.grantLive(session.token, ask.id));
            dropAsk(ask.id);
          }}
          onDecline={async () => {
            await gone(api.declineLive(session.token, ask.id));
            dropAsk(ask.id);
          }}
          onExpire={() => dropAsk(ask.id)}
        />
      )}
    </Stage>
  );
}
