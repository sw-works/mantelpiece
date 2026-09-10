import { useEffect, useState } from "react";
import { Image, Text, View } from "react-native";
import { type TvApi } from "../api/client";
import { theme } from "../theme";

/** F-207: the static route map for a trip or one of its days; blank panel until drawn. */
export function MapImage({ api, token, tripId, day, width, height }: { api: TvApi; token: string; tripId: string; day?: number; width: number; height: number }) {
  const [map, setMap] = useState<{ url: string; inferred: boolean } | null>(null);
  useEffect(() => {
    let alive = true;
    setMap(null);
    api
      .tripMap(token, tripId, day)
      .then((m) => alive && setMap(m))
      .catch(() => alive && setMap(null));
    return () => {
      alive = false;
    };
  }, [api, token, tripId, day]);
  if (!map) return <View testID="map-pending" style={{ width, height, backgroundColor: theme.panel, borderRadius: theme.radius }} />;
  return (
    <View style={{ gap: 6 }}>
      <Image testID="map" source={{ uri: map.url }} style={{ width, height, borderRadius: theme.radius }} resizeMode="cover" />
      {map.inferred && <Text style={{ color: theme.muted, fontSize: theme.small }}>Route inferred from landmarks</Text>}
    </View>
  );
}
