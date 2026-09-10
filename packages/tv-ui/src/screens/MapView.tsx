import { useEffect, useState } from "react";
import { Image, ScrollView, Text, View } from "react-native";
import { type MapView as MapViewData, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { theme } from "../theme";

/** F-707: every trip as a pin on one map, with the list beside it to open a trip. */
export function MapView({ api, token, onOpenTrip, onBack }: { api: TvApi; token: string; onOpenTrip: (tripId: string) => void; onBack: () => void }) {
  const [map, setMap] = useState<MapViewData | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    api
      .map(token)
      .then((m) => alive && setMap(m))
      .catch((e: unknown) => alive && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      alive = false;
    };
  }, [api, token]);
  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y, gap: theme.gap }}>
      <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center" }}>
        <Text style={{ color: theme.fg, fontSize: theme.title, fontWeight: "700" }}>Where you have been</Text>
        <FocusButton label="Back" onPress={onBack} testID="back" preferred={!map || map.pins.length === 0} />
      </View>
      {error && <Text testID="error" style={{ color: "#ff7a7a", fontSize: theme.body }}>{error}</Text>}
      {map && map.pins.length === 0 && <Text testID="empty" style={{ color: theme.muted, fontSize: theme.body }}>The map fills in as trips with photos on the go arrive.</Text>}
      {map?.url && (
        <View style={{ flexDirection: "row", gap: theme.gap }}>
          <Image testID="map-image" source={{ uri: map.url }} style={{ width: 1120, height: 630, borderRadius: theme.radius }} resizeMode="cover" />
          <ScrollView contentContainerStyle={{ gap: 12 }}>
            {map.pins.map((p, i) => (
              <View key={p.trip} testID={`pin-${p.trip}`} style={{ gap: 4 }}>
                <FocusButton label={p.name} preferred={i === 0} onPress={() => onOpenTrip(p.trip)} />
                <Text style={{ color: theme.muted, fontSize: theme.small }}>{p.start}</Text>
              </View>
            ))}
          </ScrollView>
        </View>
      )}
    </View>
  );
}
