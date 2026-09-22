import { useEffect, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { type PersonView, playlistAsTrip, type Trip, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { PhotoCard } from "../components/PhotoCard";
import { type Nav, Screen } from "../components/Screen";
import { Heading, Meta } from "../components/Type";
import { theme } from "../theme";

/**
 * F-704: the household's people, opt-in. The switch lives here and only here, because the
 * TV is the household: On starts grouping, Off deletes every face record. Naming happens
 * on a paired phone, where typing is easy.
 */
export function People({ api, token, onPlay, onBack, nav }: { api: TvApi; token: string; onPlay?: (trip: Trip) => void; onBack: () => void; nav?: Nav }) {
  const [state, setState] = useState<{ enabled: boolean; people: PersonView[] } | null>(null);
  const [focus, setFocus] = useState<PersonView | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    api
      .people(token)
      .then((r) => alive && setState({ enabled: r.people_enabled, people: r.people }))
      .catch((e: unknown) => alive && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      alive = false;
    };
  }, [api, token]);

  return (
    <Screen
      nav={nav}
      active="people"
      title="People"
      actions={
        <>
          {state && !state.enabled && (
            <FocusButton
              label="Turn on face grouping"
              testID="turn-on"
              onPress={() => {
                api
                  .setPeople(token, true)
                  .then(() => setState({ enabled: true, people: [] }))
                  .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
              }}
            />
          )}
          {state?.enabled && (
            <FocusButton
              label="Turn off and delete face data"
              variant="ghost"
              testID="turn-off"
              onPress={() => {
                api
                  .setPeople(token, false)
                  .then(() => setState({ enabled: false, people: [] }))
                  .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
              }}
            />
          )}
          <FocusButton label="Back" variant="ghost" onPress={onBack} preferred testID="back" />
        </>
      }
    >
      {error && <Text testID="error" style={{ color: theme.bad, fontSize: theme.body }}>{error}</Text>}
      {state && !state.enabled && (
        <Meta testID="off" size={theme.body}>
          Face grouping is off and nothing about faces is stored. Turn it on to find everyone's photos by person. Face signatures stay in a private collection for this household, and turning it off deletes them.
        </Meta>
      )}
      {state?.enabled && <Meta size={theme.body}>Grouped on this household's photos only. Name people from the People page on a paired phone.</Meta>}
      {state?.enabled && state.people.length === 0 && (
        <Meta testID="empty" size={theme.body}>
          Groups appear as trips are organised.
        </Meta>
      )}
      {state?.enabled && state.people.length > 0 && (
        <ScrollView horizontal style={{ flexGrow: 0 }} contentContainerStyle={{ gap: 44, padding: 8 }}>
          {state.people.map((p, i) => (
            <View key={p.id} testID={`person-${p.id}`} style={{ gap: 12, alignItems: "center" }}>
              <PhotoCard api={api} token={token} mediaId={p.sample_media[0]} width={200} height={200} preferred={i === 0} onPress={() => setFocus(p)} testID={`focus-${p.id}`} />
              <Text style={{ color: p.name ? theme.fg : theme.muted, fontSize: 24, fontWeight: "600" }}>{`${p.name ?? "Unnamed"} · ${p.media_count}`}</Text>
            </View>
          ))}
        </ScrollView>
      )}
      {focus && (
        <View testID="best-of" style={{ gap: 16 }}>
          <View style={{ flexDirection: "row", gap: 18, alignItems: "center" }}>
            <Heading>{`Best of ${focus.name ?? "this person"}`}</Heading>
            {onPlay && <FocusButton label="Play" testID="play-person" onPress={() => onPlay(playlistAsTrip(focus.name ?? "Best of", focus.sample_media))} />}
            <Meta>{`${focus.sample_media.length} keepers`}</Meta>
          </View>
          <ScrollView horizontal style={{ flexGrow: 0 }} contentContainerStyle={{ gap: 20, padding: 8 }}>
            {focus.sample_media.map((m) => (
              <PhotoCard key={m} api={api} token={token} mediaId={m} width={300} height={200} />
            ))}
          </ScrollView>
        </View>
      )}
    </Screen>
  );
}
