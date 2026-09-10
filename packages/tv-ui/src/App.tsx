import { useCallback, useEffect, useMemo, useState } from "react";
import { View } from "react-native";
import { type Trip, TvApi } from "./api/client";
import { Ticker } from "./components/Ticker";
import { useEvents } from "./hooks/useEvents";
import { useRemoteKeys } from "./hooks/useRemoteKeys";
import { Ask } from "./screens/Ask";
import { Highlights } from "./screens/Highlights";
import { Invite } from "./screens/Invite";
import { MemoriesHome } from "./screens/MemoriesHome";
import { Onboarding } from "./screens/Onboarding";
import { ReviewChanges } from "./screens/ReviewChanges";
import { Slideshow } from "./screens/Slideshow";
import { TripView } from "./screens/TripView";
import { MemorySessionStore, newDeviceId, type SessionStore } from "./session";

export type Route =
  | { name: "home" }
  | { name: "trip"; id: string }
  | { name: "highlights"; id: string }
  | { name: "slideshow"; trip: Trip }
  | { name: "ask" }
  | { name: "review" }
  | { name: "invite"; tripId: string | null };

export interface AppProps {
  apiBase?: string;
  api?: TvApi;
  sessionStore?: SessionStore;
  /** Name of this TV as the phone and other households see it. */
  tvName?: string;
}

const DEFAULT_API = "https://api.memorylane.test";

/** State-based router: no navigation library, no native screens module. */
export function App({ apiBase, api: injectedApi, sessionStore, tvName }: AppProps) {
  const api = useMemo(() => injectedApi ?? new TvApi(apiBase ?? DEFAULT_API), [injectedApi, apiBase]);
  const store = useMemo(() => sessionStore ?? new MemorySessionStore(), [sessionStore]);
  const [deviceId] = useState(() => newDeviceId());
  const [session, setSession] = useState<{ token: string; household: string } | null | undefined>(undefined);
  const [route, setRoute] = useState<Route>({ name: "home" });
  const feed = useEvents(api, session?.token ?? null);

  // Hardware Back mirrors the on-screen Back buttons. On the home screen it is left to the
  // platform, which exits the app.
  const goBack = useCallback(() => {
    setRoute((r) => {
      if (r.name === "trip") return { name: "home" };
      if (r.name === "highlights") return { name: "trip", id: r.id };
      if (r.name === "slideshow") return r.trip.id.startsWith("playlist:") ? { name: "ask" } : { name: "trip", id: r.trip.id };
      if (r.name === "ask" || r.name === "review") return { name: "home" };
      if (r.name === "invite") return r.tripId ? { name: "trip", id: r.tripId } : { name: "home" };
      return r;
    });
  }, []);
  // The slideshow owns the remote while it is up.
  useRemoteKeys({ onBack: route.name === "home" || route.name === "slideshow" ? undefined : goBack });

  useEffect(() => {
    store.load().then((s) => setSession(s ? { token: s.token, household: s.household } : null));
  }, [store]);

  // What is on screen, for "what's that statue" from the phone or Alexa+ (research R3).
  const token = session?.token;
  const onPresence = useCallback(
    (now: { media: string; day: number; paused: boolean }) => {
      if (!token) return;
      const trip = route.name === "slideshow" && !route.trip.id.startsWith("playlist:") ? route.trip.id : null;
      api.putPresence(token, { device_id: deviceId, media: now.media, trip, day: now.day, paused: now.paused }).catch(() => undefined);
    },
    [api, token, deviceId, route],
  );
  const pendingReviews = feed.recent.filter((e) => e.type === "dedup.proposal" || e.type === "cleanup.proposal").length;
  const lastAnswer = [...feed.recent].reverse().find((e) => e.type === "answer.card");
  const answer = lastAnswer ? { question: String(lastAnswer.question), answer: String(lastAnswer.answer), media: lastAnswer.media as string | null } : null;

  if (session === undefined) return <View style={{ flex: 1, backgroundColor: "#0b0b0f" }} />;
  if (!session) {
    return (
      <Onboarding
        api={api}
        deviceId={deviceId}
        tvName={tvName}
        onSignedIn={(s) => {
          void store.save({ ...s, deviceId });
          setSession(s);
        }}
      />
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
          onOpenTrip={(t) => setRoute({ name: "trip", id: t.id })}
          onInvite={() => setRoute({ name: "invite", tripId: null })}
          onAsk={() => setRoute({ name: "ask" })}
          onReview={() => setRoute({ name: "review" })}
          pendingReviews={pendingReviews}
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
        />
      );
      break;
    case "slideshow":
      screen = <Slideshow api={api} token={session.token} trip={route.trip} onBack={goBack} onPresence={onPresence} answer={answer} />;
      break;
    case "ask":
      screen = <Ask api={api} token={session.token} onPlay={(trip) => setRoute({ name: "slideshow", trip })} onBack={goBack} />;
      break;
    case "review":
      screen = <ReviewChanges api={api} token={session.token} onDone={goBack} />;
      break;
    case "highlights":
      screen = <Highlights api={api} token={session.token} tripId={route.id} onBack={() => setRoute({ name: "trip", id: route.id })} />;
      break;
    case "invite":
      screen = <Invite api={api} token={session.token} tripId={route.tripId} ticker={feed.ticker} onBack={() => setRoute(route.tripId ? { name: "trip", id: route.tripId } : { name: "home" })} />;
      break;
  }
  return (
    <View style={{ flex: 1 }}>
      {screen}
      {route.name !== "invite" && route.name !== "slideshow" && <Ticker line={feed.ticker} />}
    </View>
  );
}
