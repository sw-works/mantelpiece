import { useEffect, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { proposalTitle, type ProposalView, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { RemoteImage } from "../components/RemoteImage";
import { theme } from "../theme";

const SAMPLES_SHOWN = 4;

/**
 * F-410, constitution III: nothing irreversible happens until the user confirms here on
 * the remote. Every open proposal shows its before-and-after counts and sample groups
 * with reasons; Keep everything cancels it, Apply mints the token bound to it.
 */
export function ReviewChanges({ api, token, onDone }: { api: TvApi; token: string; onDone: () => void }) {
  const [proposals, setProposals] = useState<ProposalView[] | null>(null);
  const [showAll, setShowAll] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    api
      .proposals(token)
      .then((p) => alive && setProposals(p.filter((x) => x.state === "open")))
      .catch((e: unknown) => alive && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      alive = false;
    };
  }, [api, token]);

  const apply = async (p: ProposalView) => {
    setBusy(p.id);
    setError(null);
    try {
      await api.mintConfirmation(token, p.action, p.id);
      setProposals((list) => (list ?? []).filter((x) => x.id !== p.id));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };
  const keep = async (p: ProposalView) => {
    setBusy(p.id);
    try {
      await api.cancelProposal(token, p.id);
      setProposals((list) => (list ?? []).filter((x) => x.id !== p.id));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y, gap: theme.gap }}>
      <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center" }}>
        <Text style={{ color: theme.fg, fontSize: theme.title, fontWeight: "700" }}>Review changes</Text>
        <FocusButton label="Done" onPress={onDone} testID="done" preferred={proposals?.length === 0} />
      </View>
      {error && <Text testID="error" style={{ color: "#ff7a7a", fontSize: theme.body }}>{error}</Text>}
      {proposals && proposals.length === 0 && (
        <Text testID="empty" style={{ color: theme.muted, fontSize: theme.body }}>Nothing waiting for you. Everything stays as it is until you say otherwise.</Text>
      )}
      <ScrollView contentContainerStyle={{ gap: theme.gap }}>
        {proposals?.map((p, i) => {
          const samples = showAll.has(p.id) ? p.samples : p.samples.slice(0, SAMPLES_SHOWN);
          return (
            <View key={p.id} testID={`proposal-${p.id}`} style={{ backgroundColor: theme.panel, borderRadius: theme.radius, padding: 24, gap: 12 }}>
              <Text style={{ color: theme.fg, fontSize: theme.body, fontWeight: "600" }}>{proposalTitle(p)}</Text>
              <Text testID={`counts-${p.id}`} style={{ color: theme.muted, fontSize: theme.body }}>{`${p.before} before · ${p.after} after`}</Text>
              <ScrollView horizontal contentContainerStyle={{ gap: 16 }}>
                {samples.map((s, j) => (
                  <View key={j} testID={`sample-${p.id}-${j}`} style={{ gap: 6 }}>
                    <View style={{ flexDirection: "row", gap: 6 }}>
                      {s.keep && <RemoteImage api={api} token={token} mediaId={s.keep} rendition="thumb" width={160} height={110} />}
                      {s.remove.slice(0, 2).map((m) => (
                        <RemoteImage key={m} api={api} token={token} mediaId={m} rendition="thumb" width={120} height={80} />
                      ))}
                    </View>
                    <Text style={{ color: theme.muted, fontSize: theme.small }}>{`${s.keep ? "keep 1, " : ""}remove ${s.remove.length} · ${s.reason}`}</Text>
                  </View>
                ))}
              </ScrollView>
              <View style={{ flexDirection: "row", gap: 16 }}>
                <FocusButton label="Keep everything" preferred={i === 0} testID={`cancel-${p.id}`} onPress={() => void keep(p)} />
                <FocusButton label={busy === p.id ? "Confirming…" : "Apply"} testID={`apply-${p.id}`} onPress={() => void apply(p)} />
                {p.samples.length > SAMPLES_SHOWN && !showAll.has(p.id) && (
                  <FocusButton label={`Show all ${p.samples.length}`} testID={`all-${p.id}`} onPress={() => setShowAll(new Set([...showAll, p.id]))} />
                )}
              </View>
            </View>
          );
        })}
      </ScrollView>
    </View>
  );
}
