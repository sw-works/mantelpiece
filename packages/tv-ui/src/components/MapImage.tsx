import { useEffect, useRef, useState } from "react";
import { Image, Text, View } from "react-native";
import { type TvApi } from "../api/client";
import { theme } from "../theme";

/**
 * F-712: the hand-drawn sketch map of a trip or one of its days, shown whole, with its
 * map-data credit; a blank panel until drawn. `onMissing` hears when there is none.
 */
export function MapImage({
  api,
  token,
  tripId,
  day,
  width,
  height,
  credit = true,
  onMissing,
}: {
  api: TvApi;
  token: string;
  tripId: string;
  day?: number;
  width: number;
  height: number;
  credit?: boolean;
  onMissing?: () => void;
}) {
  const [map, setMap] = useState<{ url: string; inferred: boolean; credit?: string } | null>(null);
  // A notification, not an input: the latest callback without refetching when it changes.
  const missing = useRef(onMissing);
  missing.current = onMissing;
  useEffect(() => {
    let alive = true;
    setMap(null);
    api
      .tripMap(token, tripId, day)
      .then((m) => alive && setMap(m))
      .catch(() => {
        if (!alive) return;
        setMap(null);
        missing.current?.();
      });
    return () => {
      alive = false;
    };
  }, [api, token, tripId, day]);
  if (!map) return <View testID="map-pending" style={{ width, height, backgroundColor: theme.panel, borderRadius: theme.radius }} />;
  return (
    <View style={{ gap: 6 }}>
      <Image testID="map" source={{ uri: map.url }} style={{ width, height, borderRadius: theme.radius }} resizeMode="contain" />
      {credit && map.credit && <Text style={{ color: theme.muted, fontSize: theme.credit }}>{map.credit}</Text>}
      {map.inferred && <Text style={{ color: theme.muted, fontSize: theme.small }}>Part of the route was estimated from landmarks</Text>}
    </View>
  );
}
