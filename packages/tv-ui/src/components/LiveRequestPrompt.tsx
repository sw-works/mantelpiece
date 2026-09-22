import { useEffect, useState } from "react";
import { View } from "react-native";
import { type LiveAsk } from "../hooks/useEvents";
import { theme } from "../theme";
import { FocusButton } from "./FocusButton";
import { Eyebrow, Heading, Meta } from "./Type";

/**
 * A phone asks to show its camera on this TV. Nothing streams until someone here says yes;
 * when another phone is live, yes ends that broadcast. The prompt sits over whatever is on
 * screen and goes away when answered, withdrawn, or out of time.
 */
export function LiveRequestPrompt({
  ask,
  onGrant,
  onDecline,
  onExpire,
  now = Date.now,
}: {
  ask: LiveAsk;
  onGrant: () => Promise<void>;
  onDecline: () => Promise<void>;
  onExpire: () => void;
  now?: () => number;
}) {
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const left = Date.parse(ask.expiresAt) - now();
    const t = setTimeout(onExpire, Math.max(0, left));
    return () => clearTimeout(t);
  }, [ask.expiresAt, onExpire, now]);

  const answer = (act: () => Promise<void>) => async () => {
    setBusy(true);
    setFailed(false);
    try {
      await act();
    } catch {
      setFailed(true);
      setBusy(false);
    }
  };

  return (
    <View
      testID="live-request"
      style={{ position: "absolute", right: theme.safe.x, top: theme.safe.y, width: 760, padding: 36, gap: 18, borderRadius: theme.radius, backgroundColor: theme.panel, borderWidth: 2, borderColor: theme.accent }}
    >
      <Eyebrow>Live</Eyebrow>
      <Heading testID="live-request-title">{`${ask.from} wants to go live`}</Heading>
      <Meta testID="live-request-meta" size={theme.body}>
        {failed ? "That did not work. Try again." : ask.replacing ? `This ends ${ask.replacing}'s broadcast.` : "Their camera shows here, a few seconds behind."}
      </Meta>
      <View style={{ flexDirection: "row", gap: 18, marginTop: 6 }}>
        <FocusButton label={ask.replacing ? `Switch to ${ask.from}` : "Watch"} preferred disabled={busy} onPress={answer(onGrant)} testID="live-grant" />
        <FocusButton label="Not now" variant="ghost" disabled={busy} onPress={answer(onDecline)} testID="live-decline" />
      </View>
    </View>
  );
}
