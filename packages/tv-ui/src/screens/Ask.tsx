import { useState } from "react";
import { ScrollView, Text, TextInput, View } from "react-native";
import { playlistAsTrip, type SearchHit, type Trip, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { RemoteImage } from "../components/RemoteImage";
import { theme } from "../theme";

/**
 * F-303, F-405: type a question on the TV, see the matches, play them as a slideshow.
 * Voice never comes through the TV (no microphone API on Vega); it arrives from the phone
 * or Alexa+ and lands here as playlists and answer cards over the feed.
 */
export function Ask({
  api,
  token,
  onPlay,
  onBack,
}: {
  api: TvApi;
  token: string;
  onPlay: (trip: Trip) => void;
  onBack: () => void;
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchHit[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const search = async () => {
    if (!query.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const r = await api.search(token, query.trim());
      setResults(r.results);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y, gap: theme.gap }}>
      <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center" }}>
        <Text style={{ color: theme.fg, fontSize: theme.title, fontWeight: "700" }}>Ask</Text>
        <FocusButton label="Back" onPress={onBack} testID="back" />
      </View>
      <Text style={{ color: theme.muted, fontSize: theme.body }}>
        Try "the ramen place on day 1" or "beach days". Or ask out loud from your phone's Ask page.
      </Text>
      <View style={{ flexDirection: "row", gap: 16, alignItems: "center" }}>
        <TextInput
          testID="query"
          value={query}
          onChangeText={setQuery}
          onSubmitEditing={() => void search()}
          placeholder="What are you looking for?"
          placeholderTextColor={theme.muted}
          style={{ flex: 1, color: theme.fg, fontSize: theme.body, backgroundColor: theme.panel, padding: 18, borderRadius: theme.radius }}
        />
        <FocusButton label={busy ? "Searching…" : "Search"} onPress={() => void search()} preferred testID="search" />
      </View>
      {error && <Text testID="error" style={{ color: "#ff7a7a", fontSize: theme.body }}>{error}</Text>}
      {results && results.length === 0 && (
        <Text testID="nothing" style={{ color: theme.muted, fontSize: theme.body }}>
          Nothing matched. Try a place, a person or a day.
        </Text>
      )}
      {results && results.length > 0 && (
        <>
          <View style={{ flexDirection: "row", gap: 16, alignItems: "center" }}>
            <Text testID="count" style={{ color: theme.fg, fontSize: theme.body }}>{`${results.length} photos`}</Text>
            <FocusButton label="Play" testID="play" onPress={() => onPlay(playlistAsTrip(query.trim(), results.map((r) => r.media)))} />
          </View>
          <ScrollView contentContainerStyle={{ flexDirection: "row", flexWrap: "wrap", gap: 16 }}>
            {results.map((r) => (
              <View key={r.media} testID={`result-${r.media}`}>
                <RemoteImage api={api} token={token} mediaId={r.media} rendition="thumb" width={280} height={190} />
              </View>
            ))}
          </ScrollView>
        </>
      )}
    </View>
  );
}
