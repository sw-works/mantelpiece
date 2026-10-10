// upload.<domain>/trip.html?t=<trip code>: the read-only view of one trip for people who
// scanned the code but only want to look (F-159). No sign-in, no session; the code is the
// key. Reactions land here later (US10).

import { Api, type ReactionTarget, type ReactionsView, type TripView } from "../api.js";
import { loadConfig } from "../config.js";
import { fieldNoteView, withNotes } from "../fieldnote.js";
import { sketchMapView } from "../sketchmap.js";
import { latestComment, reactionLine, targetKey, VIEWER_NAME_KEY, viewerName } from "../reactions.js";
import { el, eyebrow, field, icon, link, mount, photo, topbar } from "../ui.js";

/** Lines for the day list: title, date, count and who took the photos. */
const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/** "March 25, 2025" from `2025-03-25`; anything else comes back as is. */
function longDate(value: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(String(value));
  const month = m ? MONTHS[Number(m[2]) - 1] : undefined;
  return m && month ? `${month} ${Number(m[3])}, ${m[1]}` : String(value);
}

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
    return `${d.title ?? `Day ${d.index}`} · ${longDate(d.date)} · ${d.media.length} photo${d.media.length === 1 ? "" : "s"}${who.length ? ` by ${who.join(", ")}` : ""}${d.route.inferred ? " · route inferred" : ""}`;
  });
}

/** The reaction summary as a heart icon plus text, so no emoji reaches the screen. */
function reactionNode(text: string): (Node | string)[] {
  if (!text.startsWith("❤ ")) return text ? [text] : [];
  return [icon("heart", 14), ` ${text.slice(2)}`];
}

function notice(root: HTMLElement, title: string, text?: string): void {
  mount(root, topbar(), el("div", { class: "stack" }, el("h1", {}, title), ...(text ? [el("p", { class: "lead" }, text)] : [])));
}

export async function main(root: HTMLElement, location: Location, storage: Storage): Promise<void> {
  const config = loadConfig();
  const api = new Api(config.apiBase);
  const code = new URL(location.href).searchParams.get("t");
  if (!code) {
    notice(root, "Mantelpiece", "Scan the code on the TV to see the trip.");
    return;
  }
  notice(root, "Opening the trip…");
  let view;
  try {
    view = await api.viewTrip(code);
  } catch (e) {
    notice(root, "That code did not open a trip", e instanceof Error ? e.message : String(e));
    return;
  }
  const trip = view.trip;
  let reactions: ReactionsView | null = await api.reactions(code, trip.id).catch(() => null);
  const nameInput = el("input", { type: "text", placeholder: "Your name", maxlength: "40", value: viewerName(storage) ?? "" });
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
        const thumbs = el("div", { class: "strip" });
        for (const m of d.media.slice(0, 6)) {
          const likes = reactions?.summary[targetKey({ target: "media", id: m })]?.likes ?? 0;
          const like = link(likes ? String(likes) : "", { icon: "heart", small: true });
          like.setAttribute("aria-label", "Like this photo");
          like.onclick = () => void react({ target: "media", id: m, kind: "like" });
          thumbs.append(el("figure", {}, photo(api.mediaUrlByCode(code, m, "thumb").then((r) => r.url), "strip"), like));
        }
        const likeDay = link("Like this day", { icon: "heart" });
        likeDay.onclick = () => void react({ target: "day", index: d.index, kind: "like" });
        const comment = el("input", { type: "text", placeholder: "Say something about this day", maxlength: "200" });
        comment.onkeydown = (e) => {
          if (e.key === "Enter" && comment.value.trim()) {
            void react({ target: "day", index: d.index, kind: "comment", text: comment.value.trim() }).then(() => (comment.value = ""));
          }
        };
        const latest = latestComment(reactions, key);
        // "Day 2 · 2025-11-13 · 61 photos by Maya": title on one line, the rest under it.
        const [title, ...rest] = (lines[i] ?? "").split(" · ");
        return el(
          "li",
          {},
          el("div", { class: "stack tight" }, el("span", { class: "day-title" }, title ?? ""), ...(rest.length ? [el("p", { class: "day-meta" }, rest.join(" · "))] : [])),
          thumbs,
          el("p", { class: "small reactions" }, ...reactionNode(reactionLine(reactions, key))),
          ...(latest ? [el("p", { class: "comment" }, latest)] : []),
          el("div", { class: "row" }, likeDay, el("span", { class: "grow" }, comment)),
        );
      }),
    );
  };
  renderDays();
  // F-711: field notes in their public form (a ring, never coordinates), with the credit.
  const noted = withNotes(trip.highlights, trip.field_notes);
  const notes: HTMLElement[] = noted.length
    ? [
        el(
          "section",
          { class: "stack tight" },
          el("span", { class: "day-title" }, "Field notes"),
          ...noted.slice(0, 6).map((m) =>
            fieldNoteView({
              photo: api.mediaUrlByCode(code, m, "tv").then((r) => r.url),
              layer: api.mediaUrlByCode(code, m, "fieldnote").then((r) => r.url),
              credit: trip.field_notes![m]!.credit,
            }),
          ),
        ),
      ]
    : [];
  // F-712: the public form of the trip map (no home end of the route), view only.
  const sketch: HTMLElement[] = view.map ? [sketchMapView({ map: Promise.resolve(view.map) })] : [];
  const photos = trip.days.reduce((n, d) => n + d.media.length, 0);
  mount(
    root,
    topbar(),
    el(
      "div",
      { class: "stack" },
      eyebrow(view.household_name ? `Shared by ${view.household_name}` : "Shared with you"),
      el("h1", {}, trip.name),
      el("p", { class: "lead" }, `${trip.start.slice(0, 10)} to ${trip.end.slice(0, 10)} · ${photos} photo${photos === 1 ? "" : "s"} · ${trip.days.length} day${trip.days.length === 1 ? "" : "s"}`),
    ),
    ...sketch,
    trip.narrative ? el("p", { class: "story" }, trip.narrative.text) : el("p", {}, "The story of this trip is being written."),
    field("Your name, so the TV can say who reacted", nameInput),
    days,
    ...notes,
    el("p", { class: "small" }, `${trip.highlights.length} highlight${trip.highlights.length === 1 ? "" : "s"}. Add your own photos from the Add photos code on the TV.`),
  );
}
