import { useEffect, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { type ThisPlace as Place, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { RemoteImage } from "../components/RemoteImage";
import { Eyebrow, Meta, Title } from "../components/Type";
import { useRemoteKeys } from "../hooks/useRemoteKeys";
import { theme } from "../theme";

/**
 * F-711, US6: what a field note's map is about. When the photo was taken, where, the main
 * feature with its credited encyclopedia description (never model-written), the photo's
 * own description, the map drawn large, and the trip's other photos from the same place.
 * A joined household sees "Around ..." and no coordinates; the API already decided that.
 */
export function ThisPlace({ api, token, tripId, mediaId, onBack }: { api: TvApi; token: string; tripId: string; mediaId: string; onBack: () => void }) {
  const [place, setPlace] = useState<Place | null>(null);
  const [failed, setFailed] = useState(false);
  useRemoteKeys({ onBack });
  useEffect(() => {
    let alive = true;
    api
      .place(token, tripId, mediaId)
      .then((p) => alive && setPlace(p))
      .catch(() => alive && setFailed(true));
    return () => {
      alive = false;
    };
  }, [api, token, tripId, mediaId]);

  if (!place) {
    return (
      <View testID="this-place-loading" style={{ flex: 1, backgroundColor: theme.bg, padding: theme.safe.x, justifyContent: "center", gap: theme.gap }}>
        <Meta size={theme.body}>{failed ? "This place could not be opened." : "Opening the map…"}</Meta>
        <FocusButton label="Back" onPress={onBack} preferred testID="back" />
      </View>
    );
  }
  const f = place.feature;
  return (
    <View testID="this-place" style={{ flex: 1, backgroundColor: theme.bg, flexDirection: "row", paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y, gap: 56 }}>
      <View style={{ width: 1000, justifyContent: "center" }}>
        <RemoteImage api={api} token={token} mediaId={mediaId} rendition="fieldnote_map" width={1000} height={760} radius={theme.radius} resizeMode="contain" />
        <Meta size={theme.credit} style={{ marginTop: 10 }}>{place.credit}</Meta>
      </View>
      <ScrollView style={{ flex: 1 }} contentContainerStyle={{ gap: 18, paddingVertical: 24 }}>
        <Eyebrow>This place</Eyebrow>
        {place.place && <Title testID="this-place-name">{place.place}</Title>}
        {place.taken && (
          <Meta testID="this-place-taken" size={theme.body} color={theme.fg}>
            {place.taken}
          </Meta>
        )}
        {place.coords && <Meta testID="this-place-coords">{place.coords}</Meta>}
        {f && (
          <View testID="this-place-feature" style={{ gap: 8, marginTop: 12 }}>
            <Text style={{ color: theme.accent, fontSize: theme.eyebrow, fontWeight: "700", letterSpacing: 2 }}>{`${f.name}${f.detail ? ` · ${f.detail}` : ""}`.toUpperCase()}</Text>
            {f.tagline && <Meta>{f.tagline}</Meta>}
            {f.summary && <Text style={{ color: theme.fg, fontSize: 26, lineHeight: 38 }}>{f.summary}</Text>}
            {f.summary && f.credit && <Meta size={theme.credit}>{`From ${f.credit}`}</Meta>}
          </View>
        )}
        {place.description && (
          <Text testID="this-place-description" style={{ color: theme.muted, fontSize: 24, lineHeight: 34, fontStyle: "italic" }}>
            {place.description}
          </Text>
        )}
        {place.nearby.length > 0 && (
          <View style={{ gap: 10, marginTop: 8 }}>
            <Meta>{`More from here · ${place.nearby.length}`}</Meta>
            <View testID="this-place-nearby" style={{ flexDirection: "row", flexWrap: "wrap", gap: 10 }}>
              {place.nearby.slice(0, 6).map((m) => (
                <RemoteImage key={m} api={api} token={token} mediaId={m} rendition="thumb" width={180} height={120} radius={12} />
              ))}
            </View>
          </View>
        )}
        <View style={{ marginTop: 12 }}>
          <FocusButton label="Back to the photo" onPress={onBack} preferred testID="back" />
        </View>
      </ScrollView>
    </View>
  );
}
