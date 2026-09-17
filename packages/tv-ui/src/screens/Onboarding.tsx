import { useEffect } from "react";
import { Text, View } from "react-native";
import { type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { QrView } from "../components/QrView";
import { Eyebrow, Meta, Title } from "../components/Type";
import { useDeviceSignIn } from "../hooks/useDeviceSignIn";
import { theme } from "../theme";

/**
 * F-401: first launch. The TV shows a code and a QR; the phone approves it with the
 * owner's Amazon account. Nothing to type on the remote.
 */
export function Onboarding({
  api,
  deviceId,
  tvName,
  onSignedIn,
  retryMs,
  now,
}: {
  api: TvApi;
  deviceId: string;
  tvName?: string;
  onSignedIn: (session: { token: string; household: string }) => void;
  retryMs?: number;
  now?: () => number;
}) {
  const state = useDeviceSignIn({ api, deviceId, tvName, retryMs, now });
  // Hand the session up after render, never during it: App sets state on this call and
  // React refuses a parent update while the child renders.
  const approved = state.phase === "approved" ? state : null;
  useEffect(() => {
    if (approved) onSignedIn({ token: approved.token, household: approved.household });
  }, [approved?.token]);
  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y }}>
      <Text style={{ color: theme.fg, fontSize: 30, fontWeight: "600" }}>MemoryLane</Text>
      <View style={{ flex: 1, flexDirection: "row", gap: 64, alignItems: "center" }}>
        <View style={{ flex: 1, gap: 26 }}>
          <Eyebrow>First time on this TV</Eyebrow>
          <Title size={84}>Sign in from your phone. Nothing to type on the remote.</Title>
          <Meta size={theme.body}>Scan the code, or open the link and enter the code. Your Amazon account, your household, your photos.</Meta>
          {state.phase === "waiting" && (
            <>
              <Text testID="device-code" style={{ color: theme.fg, fontSize: 128, fontWeight: "700", letterSpacing: 14, lineHeight: 140 }}>
                {`${state.code.slice(0, 4)} ${state.code.slice(4)}`}
              </Text>
              <Meta size={theme.body}>{state.link}</Meta>
            </>
          )}
          {state.phase === "issuing" && <Meta size={theme.body}>Getting a code…</Meta>}
          {state.phase === "expired" && (
            <>
              <Meta testID="expired" size={theme.body}>{`Code ${state.code.slice(0, 4)} ${state.code.slice(4)} expired. Codes last ten minutes.`}</Meta>
              <FocusButton label="Get a new code" onPress={state.restart} preferred />
            </>
          )}
          {state.phase === "error" && (
            <>
              <Text testID="error" style={{ color: theme.bad, fontSize: theme.body }}>{state.message}</Text>
              <FocusButton label="Try again" onPress={state.restart} preferred />
            </>
          )}
        </View>
        {state.phase === "waiting" && (
          <View style={{ alignItems: "center", gap: 22 }}>
            <View style={{ padding: 28, borderRadius: 28, backgroundColor: "#fff" }}>
              <QrView text={state.link} size={440} />
            </View>
            <View style={{ flexDirection: "row", alignItems: "center", gap: 10 }}>
              <View style={{ width: 12, height: 12, borderRadius: 6, backgroundColor: theme.accent }} />
              <Meta>Waiting for your phone · code lasts 10 minutes</Meta>
            </View>
          </View>
        )}
      </View>
    </View>
  );
}
