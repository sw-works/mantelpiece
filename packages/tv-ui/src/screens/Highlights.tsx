import { useEffect, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { creditFor, type Trip, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { RemoteImage } from "../components/RemoteImage";
import { theme } from "../theme";

/** F-209: the trip's highlights as a grid, each credited to whoever took it (F-154). */
export function Highlights({
  api,
  token,
  tripId,
  onBack,
}: {
  api: TvApi;
  token: string;
  tripId: string;
  onBack: () => void;
}) {
  const [trip, setTrip] = useState<Trip | null>(null);
  const [keepers, setKeepers] = useState<string[]>([]);
  useEffect(() => {
    let alive = true;
    api
      .trip(token, tripId)
      .then((t) => alive && setTrip(t))
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [api, token, tripId]);

  if (!trip) return <View style={{ flex: 1, backgroundColor: theme.bg }} />;
  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y, gap: theme.gap }}>
      <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center" }}>
        <Text style={{ color: theme.fg, fontSize: theme.title, fontWeight: "700" }}>{`${trip.name} · highlights`}</Text>
        <FocusButton label="Back" onPress={onBack} preferred testID="back" />
      </View>
      {trip.highlights.length === 0 ? (
        <Text testID="empty" style={{ color: theme.muted, fontSize: theme.body }}>
          Highlights appear once the trip is organised.
        </Text>
      ) : (
        <ScrollView contentContainerStyle={{ flexDirection: "row", flexWrap: "wrap", gap: 16 }}>
          {trip.highlights.map((m) => (
            <View key={m} testID={`highlight-${m}`} style={{ gap: 6 }}>
              <RemoteImage api={api} token={token} mediaId={m} rendition="thumb" width={400} height={225} />
              <Text style={{ color: theme.muted, fontSize: theme.small }}>{creditFor(trip, m) ?? ""}</Text>
              <FocusButton
                label={keepers.includes(m) ? "Keeper" : "Make keeper"}
                testID={`keeper-${m}`}
                onPress={() => {
                  api
                    .setKeeper(token, m)
                    .then(() => setKeepers((k) => [...k, m]))
                    .catch(() => undefined);
                }}
              />
            </View>
          ))}
        </ScrollView>
      )}
    </View>
  );
}
