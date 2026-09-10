// upload.<domain>/trip.html?t=<trip code>: the read-only view of one trip for people who
// scanned the code but only want to look (F-159). No sign-in, no session; the code is the
// key. Reactions land here later (US10).

import { Api, type ReactionTarget, type ReactionsView, type TripView } from "../api.js";
import { loadConfig } from "../config.js";
import { latestComment, reactionLine, targetKey, VIEWER_NAME_KEY, viewerName } from "../reactions.js";
import { el, mount } from "../ui.js";

/** Lines for the day list: title, date, count and who took the photos. */
export function tripLines(trip: TripView): string[] {
  const names = (media: string[]) => {
    const out: string[] = [];
    for (const m of media) {
      const c = trip.media_credits?.[m];
      const name = c ? (trip.contributor_names?.[c] ?? c) : null;
      if (name && !out.includes(name)) out.push(name);
    }
    return out;
  };
  return trip.days.map((d) => {
    const who = names(d.media);
    return `${d.title ?? `Day ${d.index}`} · ${d.date} · ${d.media.length} photo${d.media.length === 1 ? "" : "s"}${who.length ? ` by ${who.join(", ")}` : ""}${d.route.inferred ? " · route inferred" : ""}`;
  });
}

export async function main(root: HTMLElement, location: Location, storage: Storage): Promise<void> {
  const config = loadConfig();
  const api = new Api(config.apiBase);
  const code = new URL(location.href).searchParams.get("t");
  if (!code) {
    mount(root, el("h1", {}, "MemoryLane"), el("p", {}, "Scan the code on the TV to see the trip."));
    return;
  }
  mount(root, el("h1", {}, "Opening the trip…"));
  let view;
  try {
    view = await api.viewTrip(code);
  } catch (e) {
    mount(root, el("h1", {}, "That code did not open a trip"), el("p", {}, e instanceof Error ? e.message : String(e)));
    return;
  }
  const trip = view.trip;
  let reactions: ReactionsView | null = await api.reactions(code, trip.id).catch(() => null);
  const nameInput = el("input", { type: "text", placeholder: "Your name, so the TV can say who reacted", maxlength: "40", value: viewerName(storage) ?? "" });
  nameInput.onchange = () => storage.setItem(VIEWER_NAME_KEY, nameInput.value.trim());
  const days = el("ol", { class: "days" });
  const lines = tripLines(trip);
  const refresh = async () => {
    reactions = await api.reactions(code, trip.id).catch(() => reactions);
    renderDays();
  };
  const react = async (body: ReactionTarget & { kind: "like" | "comment"; text?: string }) => {
    if (!viewerName(storage)) {
      nameInput.focus();
      nameInput.placeholder = "Tell everyone who you are first";
      return;
    }
    await api.react(code, trip.id, { ...body, name: viewerName(storage)! });
    await refresh();
  };
  const renderDays = () => {
    days.replaceChildren(
      ...trip.days.map((d, i) => {
        const key = targetKey({ target: "day", index: d.index });
        const thumbs = el("div", { class: "thumbs" });
        for (const m of d.media.slice(0, 6)) {
          const img = el("img", { alt: "", loading: "lazy" });
          api
            .mediaUrlByCode(code, m, "thumb")
            .then((r) => (img.src = r.url))
            .catch(() => img.remove());
          const like = el("button", { class: "link" }, `❤ ${reactions?.summary[targetKey({ target: "media", id: m })]?.likes ?? ""}`.trim());
          like.onclick = () => void react({ target: "media", id: m, kind: "like" });
          thumbs.append(el("figure", {}, img, like));
        }
        const likeDay = el("button", { class: "link" }, "Like this day");
        likeDay.onclick = () => void react({ target: "day", index: d.index, kind: "like" });
        const comment = el("input", { type: "text", placeholder: "Say something about this day", maxlength: "200" });
        comment.onkeydown = (e) => {
          if (e.key === "Enter" && comment.value.trim()) {
            void react({ target: "day", index: d.index, kind: "comment", text: comment.value.trim() }).then(() => (comment.value = ""));
          }
        };
        const latest = latestComment(reactions, key);
        return el(
          "li",
          {},
          el("p", {}, lines[i] ?? ""),
          thumbs,
          el("p", { class: "muted reactions" }, reactionLine(reactions, key)),
          ...(latest ? [el("p", { class: "story" }, latest)] : []),
          likeDay,
          comment,
        );
      }),
    );
  };
  renderDays();
  mount(
    root,
    el("h1", {}, trip.name),
    el("p", { class: "muted" }, `${trip.start.slice(0, 10)} to ${trip.end.slice(0, 10)}${view.household_name ? ` · shared by ${view.household_name}` : ""}`),
    trip.narrative ? el("p", { class: "story" }, trip.narrative.text) : el("p", { class: "muted" }, "The story of this trip is being written."),
    el("h2", {}, "Days"),
    nameInput,
    days,
    el("p", { class: "muted" }, `${trip.highlights.length} highlights. Add your own photos from the upload link on the TV.`),
  );
}
