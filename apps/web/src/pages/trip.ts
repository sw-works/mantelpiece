// upload.<domain>/trip.html?t=<trip code>: the read-only view of one trip for people who
// scanned the code but only want to look (F-159). No sign-in, no session; the code is the
// key. Reactions land here later (US10).

import { Api, type TripView } from "../api.js";
import { loadConfig } from "../config.js";
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

export async function main(root: HTMLElement, location: Location, _storage: Storage): Promise<void> {
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
  const days = el("ol", { class: "days" });
  const lines = tripLines(trip);
  trip.days.forEach((d, i) => {
    const thumbs = el("div", { class: "thumbs" });
    for (const m of d.media.slice(0, 6)) {
      const img = el("img", { alt: "", loading: "lazy" });
      api
        .mediaUrlByCode(code, m, "thumb")
        .then((r) => (img.src = r.url))
        .catch(() => img.remove());
      thumbs.append(img);
    }
    days.append(el("li", {}, el("p", {}, lines[i] ?? ""), thumbs));
  });
  mount(
    root,
    el("h1", {}, trip.name),
    el("p", { class: "muted" }, `${trip.start.slice(0, 10)} to ${trip.end.slice(0, 10)}${view.household_name ? ` · shared by ${view.household_name}` : ""}`),
    trip.narrative ? el("p", { class: "story" }, trip.narrative.text) : el("p", { class: "muted" }, "The story of this trip is being written."),
    el("h2", {}, "Days"),
    days,
    el("p", { class: "muted" }, `${trip.highlights.length} highlights. Add your own photos from the upload link on the TV.`),
  );
}
