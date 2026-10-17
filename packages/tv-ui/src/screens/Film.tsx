import { useEffect, useState } from "react";
import { View } from "react-native";
import { type FilmInfo, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { Eyebrow, Meta, Title } from "../components/Type";
import { type FilmPlayer, type FilmPlayerState } from "../film";
import { useRemoteKeys } from "../hooks/useRemoteKeys";
import { theme } from "../theme";

/**
 * F-714: the film of one trip day, rendered in the cloud, full screen. The picture is the
 * platform's player; this screen finds the film, says what is happening, and offers Back
 * (and Play again once it has ended).
 */
export function Film({ api, token, tripId, tripName, day, player: Player, onBack }: { api: TvApi; token: string; tripId: string; tripName: string; day: number; player?: FilmPlayer; onBack: () => void }) {
  // undefined while asking; null when the day has no film yet.
  const [film, setFilm] = useState<FilmInfo | null | undefined>(undefined);
  const [failed, setFailed] = useState(false);
  const [state, setState] = useState<FilmPlayerState>("starting");
  const [attempt, setAttempt] = useState(0);
  useRemoteKeys({ onBack });
  useEffect(() => {
    if (!Player) return;
    let alive = true;
    setFilm(undefined);
    setFailed(false);
    api
      .film(token, tripId, day)
      .then((f) => alive && setFilm(f))
      .catch(() => {
        if (!alive) return;
        setFilm(null);
        setFailed(true);
      });
    return () => {
      alive = false;
    };
  }, [api, token, tripId, day, Player]);

  const heading = `${tripName} · Day ${day}`;
  if (!Player) return <Notice testID="no-film-player" heading={heading} line="This TV cannot play films yet." onBack={onBack} />;
  if (film === undefined) return <Notice testID="film-loading" heading={heading} line="Finding the film…" onBack={onBack} />;
  if (film === null) return <Notice testID="no-film" heading={heading} line={failed ? "The film could not be found. Try again in a moment." : `There is no film of day ${day} yet.`} onBack={onBack} />;

  const line = { starting: "Starting the film…", playing: null, ended: "The end", error: "The film could not play." }[state];
  return (
    <View testID="film" style={{ flex: 1, backgroundColor: "#000" }}>
      {state !== "ended" && <Player key={attempt} uri={film.url} onState={setState} />}
      {line && (
        <View style={{ position: "absolute", left: theme.safe.x, top: theme.safe.y, paddingHorizontal: 20, height: 56, justifyContent: "center", borderRadius: 28, backgroundColor: "#0b0b0fd9" }}>
          <Meta testID="film-state" color={theme.fg} size={theme.body}>{`${heading} · ${line}`}</Meta>
        </View>
      )}
      <View style={{ position: "absolute", right: theme.safe.x, bottom: theme.safe.y, flexDirection: "row", gap: 14 }}>
        {state === "ended" && (
          <FocusButton
            label="Play again"
            preferred
            testID="film-again"
            onPress={() => {
              setState("starting");
              setAttempt((n) => n + 1);
            }}
          />
        )}
        <FocusButton label="Back" variant="ghost" onPress={onBack} preferred={state !== "ended"} testID="back" />
      </View>
    </View>
  );
}

function Notice({ testID, heading, line, onBack }: { testID: string; heading: string; line: string; onBack: () => void }) {
  return (
    <View testID={testID} style={{ flex: 1, backgroundColor: "#000", paddingHorizontal: theme.safe.x, justifyContent: "center", gap: theme.gap }}>
      <Eyebrow>Day film</Eyebrow>
      <Title>{heading}</Title>
      <Meta testID={`${testID}-line`} size={theme.body}>
        {line}
      </Meta>
      <FocusButton label="Back" onPress={onBack} preferred testID="back" />
    </View>
  );
}
