import { useCallback, useEffect, useMemo, useState } from "react";
import { View } from "react-native";
import { TvApi } from "./api/client";
import { Ticker } from "./components/Ticker";
import { useEvents } from "./hooks/useEvents";
import { useRemoteKeys } from "./hooks/useRemoteKeys";
import { Highlights } from "./screens/Highlights";
import { Invite } from "./screens/Invite";
import { MemoriesHome } from "./screens/MemoriesHome";
import { Onboarding } from "./screens/Onboarding";
import { TripView } from "./screens/TripView";
import { MemorySessionStore, newDeviceId, type SessionStore } from "./session";

export type Route = { name: "home" } | { name: "trip"; id: string } | { name: "highlights"; id: string } | { name: "invite"; tripId: string | null };

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
      if (r.name === "invite") return r.tripId ? { name: "trip", id: r.tripId } : { name: "home" };
      return r;
    });
  }, []);
  useRemoteKeys({ onBack: route.name === "home" ? undefined : goBack });

  useEffect(() => {
    store.load().then((s) => setSession(s ? { token: s.token, household: s.household } : null));
  }, [store]);

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
        />
      );
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
      {route.name !== "invite" && <Ticker line={feed.ticker} />}
    </View>
  );
}
