import { useEffect, useState } from "react";
import { Image, ScrollView, Text, View } from "react-native";
import { type MapView as MapViewData, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { type Nav, Screen } from "../components/Screen";
import { Meta } from "../components/Type";
import { theme } from "../theme";

/** F-707: every trip as a pin on one map, with the list beside it to open a trip. */
export function MapView({ api, token, onOpenTrip, onBack, nav }: { api: TvApi; token: string; onOpenTrip: (tripId: string) => void; onBack: () => void; nav?: Nav }) {
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
    <Screen nav={nav} active="map" title="Where you have been" actions={<FocusButton label="Back" variant="ghost" onPress={onBack} testID="back" preferred={!map || map.pins.length === 0} />}>
      {error && <Text testID="error" style={{ color: theme.bad, fontSize: theme.body }}>{error}</Text>}
      {map && map.pins.length === 0 && (
        <Meta testID="empty" size={theme.body}>
          The map fills in as trips with photos on the go arrive.
        </Meta>
      )}
      {map?.url && (
        <View style={{ flex: 1, flexDirection: "row", gap: 36 }}>
          <Image testID="map-image" source={{ uri: map.url }} style={{ width: 1080, height: 870, borderRadius: 22 }} resizeMode="cover" />
          <ScrollView contentContainerStyle={{ gap: 10 }}>
            <Meta>{`${map.pins.length} trip${map.pins.length === 1 ? "" : "s"}`}</Meta>
            {map.pins.map((p, i) => (
              <View key={p.trip} testID={`pin-${p.trip}`} style={{ gap: 4 }}>
                <FocusButton label={p.name} variant="ghost" preferred={i === 0} onPress={() => onOpenTrip(p.trip)} />
                <Meta size={theme.credit} style={{ paddingLeft: 30 }}>
                  {p.start}
                </Meta>
              </View>
            ))}
          </ScrollView>
        </View>
      )}
    </Screen>
  );
}
