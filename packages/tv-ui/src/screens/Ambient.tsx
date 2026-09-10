import { useEffect, useState } from "react";
import { Text, View } from "react-native";
import { playlistAsTrip, type Trip, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { theme } from "../theme";
import { Slideshow, type NowShowing } from "./Slideshow";

/**
 * F-406: what the TV shows when nobody is pressing buttons. Plays the Concierge's
 * ambient playlist (W3), which the API already returns with this household's pins first
 * and hidden items gone, at a slower pace. Any key returns home.
 */
export function Ambient({ api, token, onBack, onPresence, intervalMs = 10000 }: { api: TvApi; token: string; onBack: () => void; onPresence?: (now: NowShowing) => void; intervalMs?: number }) {
  const [trip, setTrip] = useState<Trip | null | undefined>(undefined);
  useEffect(() => {
    let alive = true;
    (async () => {
      const lists = await api.playlists(token);
      const ambient = lists.find((p) => p.source.startsWith("ambient")) ?? lists[0];
      if (!ambient) return alive && setTrip(null);
      const full = await api.playlist(token, ambient.id);
      if (alive) setTrip(full.media.length ? playlistAsTrip("Ambient", full.media) : null);
    })().catch(() => alive && setTrip(null));
    return () => {
      alive = false;
    };
  }, [api, token]);
  if (trip === undefined) return <View style={{ flex: 1, backgroundColor: "#000" }} />;
  if (trip === null) {
    return (
      <View style={{ flex: 1, backgroundColor: theme.bg, padding: theme.safe.x, justifyContent: "center", gap: theme.gap }}>
        <Text testID="empty" style={{ color: theme.muted, fontSize: theme.body }}>Nothing to show yet. The ambient set appears after the first night's tidy-up.</Text>
        <FocusButton label="Back" onPress={onBack} preferred testID="back" />
      </View>
    );
  }
  return <Slideshow api={api} token={token} trip={trip} onBack={onBack} onPresence={onPresence} intervalMs={intervalMs} />;
}
