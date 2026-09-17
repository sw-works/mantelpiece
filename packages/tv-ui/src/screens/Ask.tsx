import { useState } from "react";
import { ScrollView, Text, TextInput, View } from "react-native";
import { playlistAsTrip, type SearchHit, type Trip, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { PhotoCard } from "../components/PhotoCard";
import { type Nav, Screen } from "../components/Screen";
import { Heading, Meta } from "../components/Type";
import { theme } from "../theme";

const SUGGESTIONS = ["beach days", "the ramen place on day 1", "sunsets"];

/**
 * F-303, F-405: type a question on the TV, see the matches, play them as a slideshow.
 * Voice never comes through the TV (no microphone API on Vega); it arrives from the phone
 * or Alexa+ and lands here as playlists and answer cards over the feed.
 */
export function Ask({ api, token, onPlay, onBack, nav }: { api: TvApi; token: string; onPlay: (trip: Trip) => void; onBack: () => void; nav?: Nav }) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchHit[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [focused, setFocused] = useState(false);

  const search = async (q = query) => {
    if (!q.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const r = await api.search(token, q.trim());
      setResults(r.results);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Screen nav={nav} active="ask" title="Ask" actions={<FocusButton label="Back" variant="ghost" onPress={onBack} testID="back" />}>
      <View style={{ flexDirection: "row", alignItems: "center", gap: 18, height: 88, paddingHorizontal: 30, borderRadius: 44, backgroundColor: theme.panel, borderWidth: 3, borderColor: focused ? theme.accent : "transparent" }}>
        <TextInput
          testID="query"
          value={query}
          onChangeText={setQuery}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          onSubmitEditing={() => void search()}
          placeholder="What are you looking for?"
          placeholderTextColor={theme.dim}
          style={{ flex: 1, color: theme.fg, fontSize: 32, padding: 0 }}
        />
        <Meta size={theme.eyebrow}>Or ask out loud from your phone</Meta>
        <FocusButton label={busy ? "Searching…" : "Search"} onPress={() => void search()} preferred testID="search" />
      </View>
      <View style={{ flexDirection: "row", gap: 10 }}>
        {SUGGESTIONS.map((s) => (
          <FocusButton
            key={s}
            variant="chip"
            label={s}
            onPress={() => {
              setQuery(s);
              void search(s);
            }}
          />
        ))}
      </View>
      {error && <Text testID="error" style={{ color: theme.bad, fontSize: theme.body }}>{error}</Text>}
      {results && results.length === 0 && (
        <Meta testID="nothing" size={theme.body}>
          Nothing matched. Try a place, a person or a day.
        </Meta>
      )}
      {results && results.length > 0 && (
        <>
          <View style={{ flexDirection: "row", gap: 18, alignItems: "center" }}>
            <Heading testID="count">{`${results.length} photos`}</Heading>
            <FocusButton label="Play as a show" testID="play" onPress={() => onPlay(playlistAsTrip(query.trim(), results.map((r) => r.media)))} />
          </View>
          <ScrollView contentContainerStyle={{ flexDirection: "row", flexWrap: "wrap", gap: 24, padding: 6 }}>
            {results.map((r) => (
              <View key={r.media} testID={`result-${r.media}`}>
                <PhotoCard api={api} token={token} mediaId={r.media} width={330} height={210} />
              </View>
            ))}
          </ScrollView>
        </>
      )}
    </Screen>
  );
}
