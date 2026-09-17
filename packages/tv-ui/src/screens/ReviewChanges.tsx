import { useEffect, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { proposalTitle, type ProposalView, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { RemoteImage } from "../components/RemoteImage";
import { type Nav, Screen } from "../components/Screen";
import { Meta } from "../components/Type";
import { theme } from "../theme";

const SAMPLES_SHOWN = 4;

/**
 * F-410, constitution III: nothing irreversible happens until the user confirms here on
 * the remote. Every open proposal shows its before-and-after counts and sample groups
 * with reasons; Keep everything cancels it, Apply mints the token bound to it.
 */
export function ReviewChanges({ api, token, onDone, nav }: { api: TvApi; token: string; onDone: () => void; nav?: Nav }) {
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
    <Screen nav={nav} active="review" title="Review changes" actions={<FocusButton label="Done" variant="ghost" onPress={onDone} testID="done" preferred={proposals?.length === 0} />}>
      {error && <Text testID="error" style={{ color: theme.bad, fontSize: theme.body }}>{error}</Text>}
      {proposals && proposals.length === 0 && (
        <Meta testID="empty" size={theme.body}>
          Nothing waiting for you. Everything stays as it is until you say otherwise.
        </Meta>
      )}
      <ScrollView contentContainerStyle={{ gap: 22 }}>
        {proposals?.map((p, i) => {
          const samples = showAll.has(p.id) ? p.samples : p.samples.slice(0, SAMPLES_SHOWN);
          return (
            <View key={p.id} testID={`proposal-${p.id}`} style={{ backgroundColor: theme.panel, borderRadius: 22, padding: 28, gap: 18, borderWidth: 2, borderColor: i === 0 ? "#ffffff22" : "transparent" }}>
              <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center", gap: 16 }}>
                <Text style={{ color: theme.fg, fontSize: 32, fontWeight: "600", flex: 1 }}>{proposalTitle(p)}</Text>
                {p.samples.length > SAMPLES_SHOWN && !showAll.has(p.id) && <FocusButton variant="chip" label={`Show all ${p.samples.length}`} testID={`all-${p.id}`} onPress={() => setShowAll(new Set([...showAll, p.id]))} />}
              </View>
              <View style={{ flexDirection: "row", gap: 40, alignItems: "center" }}>
                <View>
                  <Text style={{ color: theme.fg, fontSize: 56, fontWeight: "700", lineHeight: 60 }}>{p.before}</Text>
                  <Meta size={theme.credit}>before</Meta>
                </View>
                <View style={{ width: 60, height: 2, backgroundColor: theme.dim }} />
                <View>
                  <Text style={{ color: theme.accent, fontSize: 56, fontWeight: "700", lineHeight: 60 }}>{p.after}</Text>
                  <Meta size={theme.credit}>after</Meta>
                </View>
                <ScrollView horizontal style={{ flexGrow: 0 }} contentContainerStyle={{ gap: 22, paddingLeft: 20 }}>
                  {samples.map((s, j) => (
                    <View key={j} testID={`sample-${p.id}-${j}`} style={{ gap: 8 }}>
                      <View style={{ flexDirection: "row", gap: 6, alignItems: "flex-end" }}>
                        {s.keep && (
                          <View>
                            <RemoteImage api={api} token={token} mediaId={s.keep} rendition="thumb" width={150} height={104} radius={10} />
                            <View style={{ position: "absolute", left: 8, top: 8, paddingHorizontal: 8, paddingVertical: 3, borderRadius: 8, backgroundColor: theme.ok }}>
                              <Text style={{ color: "#06210f", fontSize: theme.credit, fontWeight: "700" }}>KEEP</Text>
                            </View>
                          </View>
                        )}
                        {s.remove.slice(0, 2).map((m) => (
                          <View key={m} style={{ opacity: 0.55 }}>
                            <RemoteImage api={api} token={token} mediaId={m} rendition="thumb" width={110} height={78} radius={10} />
                          </View>
                        ))}
                      </View>
                      <Meta size={theme.credit}>{`${s.keep ? "keep 1, " : ""}remove ${s.remove.length} · ${s.reason}`}</Meta>
                    </View>
                  ))}
                </ScrollView>
              </View>
              <Meta testID={`counts-${p.id}`} size={theme.credit}>{`${p.before} before · ${p.after} after`}</Meta>
              <View style={{ flexDirection: "row", gap: 12, alignItems: "center" }}>
                <FocusButton label="Keep everything" preferred={i === 0} testID={`cancel-${p.id}`} onPress={() => void keep(p)} />
                <FocusButton label={busy === p.id ? "Confirming…" : "Apply"} variant="ghost" testID={`apply-${p.id}`} onPress={() => void apply(p)} />
                <Meta size={theme.eyebrow}>Nothing is deleted until you press Apply here, on the remote.</Meta>
              </View>
            </View>
          );
        })}
      </ScrollView>
    </Screen>
  );
}

