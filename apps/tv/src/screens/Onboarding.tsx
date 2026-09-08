import { Text, View } from "react-native";
import { type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { QrView } from "../components/QrView";
import { useDeviceSignIn } from "../hooks/useDeviceSignIn";
import { theme } from "../theme";

export function Onboarding({
  api,
  deviceId,
  onSignedIn,
  retryMs,
}: {
  api: TvApi;
  deviceId: string;
  onSignedIn: (session: { token: string; household: string }) => void;
  retryMs?: number;
}) {
  const state = useDeviceSignIn({ api, deviceId, retryMs });
  if (state.phase === "approved") {
    onSignedIn({ token: state.token, household: state.household });
  }
  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, padding: 64, flexDirection: "row", gap: 64, alignItems: "center" }}>
      <View style={{ flex: 1, gap: theme.gap }}>
        <Text style={{ color: theme.fg, fontSize: theme.title, fontWeight: "700" }}>Sign in from your phone</Text>
        <Text style={{ color: theme.muted, fontSize: theme.body }}>
          Scan the code, or open the link and type the code. Sign in with your Amazon account. Nothing to type on the remote.
        </Text>
        {state.phase === "waiting" && (
          <>
            <Text testID="device-code" style={{ color: theme.accent, fontSize: 96, fontWeight: "800", letterSpacing: 8 }}>
              {`${state.code.slice(0, 4)} ${state.code.slice(4)}`}
            </Text>
            <Text style={{ color: theme.muted, fontSize: theme.small }}>{state.link}</Text>
          </>
        )}
        {state.phase === "issuing" && <Text style={{ color: theme.muted, fontSize: theme.body }}>Getting a code…</Text>}
        {state.phase === "error" && (
          <>
            <Text testID="error" style={{ color: "#ff7a7a", fontSize: theme.body }}>{state.message}</Text>
            <FocusButton label="Try again" onPress={state.restart} preferred />
          </>
        )}
      </View>
      {state.phase === "waiting" && <QrView text={state.link} />}
    </View>
  );
}
