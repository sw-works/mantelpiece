import { useEffect, useState } from "react";
import { ScrollView, View } from "react-native";
import { creditFor, type Trip, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { PhotoCard } from "../components/PhotoCard";
import { type Nav, Screen } from "../components/Screen";
import { Meta } from "../components/Type";
import { theme } from "../theme";

/** F-209: the trip's highlights as a grid, each credited to whoever took it (F-154). */
export function Highlights({ api, token, tripId, onBack, onPlay, nav }: { api: TvApi; token: string; tripId: string; onBack: () => void; onPlay?: (trip: Trip) => void; nav?: Nav }) {
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

  if (!trip) return <Screen nav={nav} active="home" />;
  return (
    <Screen
      nav={nav}
      active="home"
      title={`${trip.name} · highlights`}
      actions={
        <>
          {onPlay && <FocusButton label="Play" onPress={() => onPlay(trip)} testID="play" />}
          <FocusButton label="Back" variant="ghost" onPress={onBack} preferred testID="back" />
        </>
      }
    >
      {trip.highlights.length === 0 ? (
        <Meta testID="empty" size={theme.body}>
          Highlights appear once the trip is organised.
        </Meta>
      ) : (
        <>
          <Meta>{`${trip.highlights.length} highlights picked by score · press Select to make a keeper. Keepers are yours; other households keep their own.`}</Meta>
          <ScrollView contentContainerStyle={{ flexDirection: "row", flexWrap: "wrap", gap: 24, padding: 6 }}>
            {trip.highlights.map((m) => (
              <View key={m} testID={`highlight-${m}`} style={{ gap: 8 }}>
                <PhotoCard
                  api={api}
                  token={token}
                  mediaId={m}
                  width={350}
                  height={197}
                  badge={keepers.includes(m) ? "Keeper" : undefined}
                  testID={`keeper-${m}`}
                  onPress={() => {
                    api
                      .setKeeper(token, m)
                      .then(() => setKeepers((k) => [...k, m]))
                      .catch(() => undefined);
                  }}
                />
                <Meta size={theme.credit}>{creditFor(trip, m) ?? ""}</Meta>
              </View>
            ))}
          </ScrollView>
        </>
      )}
    </Screen>
  );
}
