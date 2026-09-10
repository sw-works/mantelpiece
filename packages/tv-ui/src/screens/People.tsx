import { useEffect, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { playlistAsTrip, type PersonView, type Trip, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { RemoteImage } from "../components/RemoteImage";
import { theme } from "../theme";

/**
 * F-704: the household's people, opt-in. Clusters with a sample face, a best-of row for
 * the person in focus, and Turn off, which deletes every face record. Naming happens on
 * the phone, where typing is easy.
 */
export function People({ api, token, onPlay, onBack }: { api: TvApi; token: string; onPlay?: (trip: Trip) => void; onBack: () => void }) {
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
    <View style={{ flex: 1, backgroundColor: theme.bg, paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y, gap: theme.gap }}>
      <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center" }}>
        <Text style={{ color: theme.fg, fontSize: theme.title, fontWeight: "700" }}>People</Text>
        <View style={{ flexDirection: "row", gap: 16 }}>
          {state?.enabled && (
            <FocusButton
              label="Turn off"
              testID="turn-off"
              onPress={() => {
                api
                  .setPeople(token, false)
                  .then(() => setState({ enabled: false, people: [] }))
                  .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
              }}
            />
          )}
          <FocusButton label="Back" onPress={onBack} preferred testID="back" />
        </View>
      </View>
      {error && <Text testID="error" style={{ color: "#ff7a7a", fontSize: theme.body }}>{error}</Text>}
      {state && !state.enabled && (
        <Text testID="off" style={{ color: theme.muted, fontSize: theme.body }}>
          Face grouping is off. Turn it on from the People page on your phone; nothing about faces is stored until you do.
        </Text>
      )}
      {state?.enabled && state.people.length === 0 && (
        <Text testID="empty" style={{ color: theme.muted, fontSize: theme.body }}>Groups appear as trips are organised.</Text>
      )}
      {state?.enabled && state.people.length > 0 && (
        <ScrollView contentContainerStyle={{ flexDirection: "row", flexWrap: "wrap", gap: 16 }}>
          {state.people.map((p, i) => (
            <View key={p.id} testID={`person-${p.id}`} style={{ gap: 6, alignItems: "center" }}>
              {p.sample_media[0] && <RemoteImage api={api} token={token} mediaId={p.sample_media[0]} rendition="thumb" width={200} height={200} />}
              <FocusButton label={`${p.name ?? "Unnamed"} · ${p.media_count}`} preferred={i === 0} onPress={() => setFocus(p)} testID={`focus-${p.id}`} />
            </View>
          ))}
        </ScrollView>
      )}
      {focus && (
        <View testID="best-of" style={{ gap: 8 }}>
          <View style={{ flexDirection: "row", gap: 16, alignItems: "center" }}>
            <Text style={{ color: theme.fg, fontSize: 34, fontWeight: "600" }}>{`Best of ${focus.name ?? "this person"}`}</Text>
            {onPlay && <FocusButton label="Play" testID="play-person" onPress={() => onPlay(playlistAsTrip(focus.name ?? "Best of", focus.sample_media))} />}
          </View>
          <ScrollView horizontal contentContainerStyle={{ gap: 16 }}>
            {focus.sample_media.map((m) => (
              <RemoteImage key={m} api={api} token={token} mediaId={m} rendition="thumb" width={280} height={190} />
            ))}
          </ScrollView>
        </View>
      )}
    </View>
  );
}
