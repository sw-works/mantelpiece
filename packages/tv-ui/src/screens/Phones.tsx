import { ScrollView, Text, View } from "react-native";
import { type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { QrView } from "../components/QrView";
import { Eyebrow, Heading, Meta, Title } from "../components/Type";
import { usePairing } from "../hooks/usePairing";
import { theme } from "../theme";

/**
 * Pair a phone, the way Bluetooth devices pair: the phone scans the code and gives a
 * name, the name waits here, and Allow on the remote lets it in. Also the list of phones
 * already let in, each with Forget.
 */
export function Phones({
  api,
  token,
  deviceId,
  householdName,
  version,
  first = false,
  onDone,
}: {
  api: TvApi;
  token: string;
  deviceId: string;
  householdName: string;
  /** Bumps when the feed says a phone asked. */
  version: number;
  /** Straight after first launch: nothing to go back to yet. */
  first?: boolean;
  onDone: () => void;
}) {
  const { code, queue, error, decide } = usePairing({ api, token, deviceId, version });
  const waiting = queue.pending.length;
  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y }}>
      <Text style={{ color: theme.fg, fontSize: 30, fontWeight: "600" }}>MemoryLane</Text>
      <View style={{ flex: 1, flexDirection: "row", gap: 72, alignItems: "center" }}>
        <View style={{ flex: 1, gap: 22 }}>
          <Eyebrow>{householdName}</Eyebrow>
          <Title size={72}>{waiting > 0 ? (waiting === 1 ? "A phone wants to connect." : `${waiting} phones want to connect.`) : queue.phones.length > 0 ? "Connect another phone." : "Connect a phone to add photos."}</Title>
          {waiting === 0 && <Meta size={theme.body}>Scan the code with the phone's camera and say who you are. The name shows up here; Allow lets it in. No account, no password.</Meta>}
          <ScrollView style={{ flexGrow: 0, maxHeight: 420 }} contentContainerStyle={{ gap: 14 }}>
            {queue.pending.map((p, i) => (
              <View key={p.id} testID={`pending-${p.id}`} style={{ flexDirection: "row", alignItems: "center", gap: 20, padding: 20, borderRadius: 24, backgroundColor: theme.panel }}>
                <View style={{ width: 14, height: 14, borderRadius: 7, backgroundColor: theme.accent }} />
                <Text numberOfLines={1} style={{ flex: 1, color: theme.fg, fontSize: 36, fontWeight: "600" }}>{p.name}</Text>
                <FocusButton testID={`allow-${p.id}`} label="Allow" onPress={() => void decide(p.id, true)} preferred={i === 0} />
                <FocusButton testID={`deny-${p.id}`} label="Not now" variant="ghost" onPress={() => void decide(p.id, false)} />
              </View>
            ))}
          </ScrollView>
          {queue.phones.length > 0 && (
            <View style={{ gap: 12 }}>
              <Heading>Connected</Heading>
              <ScrollView horizontal style={{ flexGrow: 0 }} contentContainerStyle={{ gap: 14 }}>
                {queue.phones.map((p) => (
                  <View key={p.id} testID={`phone-${p.id}`} style={{ flexDirection: "row", alignItems: "center", gap: 14, paddingLeft: 22, paddingRight: 8, paddingVertical: 8, borderRadius: 40, backgroundColor: theme.panel }}>
                    <View style={{ width: 12, height: 12, borderRadius: 6, backgroundColor: theme.ok }} />
                    <Text style={{ color: theme.fg, fontSize: theme.body }}>{p.name}</Text>
                    <FocusButton testID={`forget-${p.id}`} label="Forget" variant="chip" onPress={() => void decide(p.id, false)} />
                  </View>
                ))}
              </ScrollView>
            </View>
          )}
          {error && <Text testID="error" style={{ color: theme.bad, fontSize: theme.small }}>{error}</Text>}
          <View style={{ flexDirection: "row", gap: 20, marginTop: 8 }}>
            <FocusButton testID="phones-done" label={first && queue.phones.length === 0 ? "Skip for now" : first ? "Go to Memories" : "Done"} variant={waiting > 0 ? "ghost" : "solid"} onPress={onDone} preferred={waiting === 0 && queue.phones.length > 0} />
          </View>
        </View>
        <View style={{ alignItems: "center", gap: 22 }}>
          <View style={{ padding: 28, borderRadius: 28, backgroundColor: "#fff", width: 496, height: 496, alignItems: "center", justifyContent: "center" }}>
            {code && <QrView text={code.link} size={440} />}
          </View>
          {code ? (
            <>
              <Text testID="pair-code" style={{ color: theme.fg, fontSize: 56, fontWeight: "700", letterSpacing: 8 }}>{`${code.code.slice(0, 4)} ${code.code.slice(4)}`}</Text>
              <Meta>{code.link}</Meta>
            </>
          ) : (
            <Meta>Getting a code…</Meta>
          )}
        </View>
      </View>
    </View>
  );
}
