// upload.<domain>/trips.html: every trip in the household for the owner's phone, grouped
// by year; open one to see its days and first photos. Needs the owner session from the
// link page, the same as People and Ask.

import { Api, type TripView } from "../api.js";
import { OWNER_TOKEN_KEY } from "../auth.js";
import { loadConfig } from "../config.js";
import { tripLines } from "./trip.js";
import { castConfirmation, groupByYear, type NowShowing, tripSummaryLine, untilConfirmed } from "../trips.js";
import { el, mount } from "../ui.js";

export async function main(root: HTMLElement, _location: Location, storage: Storage): Promise<void> {
  const config = loadConfig();
  const api = new Api(config.apiBase);
  const token = storage.getItem(OWNER_TOKEN_KEY);
  if (!token) {
    mount(root, el("h1", {}, "My trips"), el("p", {}, "Sign in your TV first: scan the code on the TV with this phone."));
    return;
  }
  mount(root, el("h1", {}, "My trips"), el("p", {}, "Loading…"));
  const trips = await api.trips(token);
  if (trips.length === 0) {
    mount(root, el("h1", {}, "My trips"), el("p", {}, "No trips yet. Press Add photos on the TV and scan the code."));
    return;
  }
  const groups = groupByYear(trips);
  const sections: HTMLElement[] = [];
  for (const group of groups) {
    const list = el("ul", { class: "uploads" });
    for (const trip of group.trips) list.append(tripRow(api, token, trip));
    sections.push(el("section", { class: "year" }, ...(groups.length > 1 ? [el("h2", {}, group.year)] : []), list));
  }
  const jump = groups.length > 1 ? el("p", { class: "jump" }, ...groups.map((g) => el("a", { href: `#year-${g.year}` }, g.year))) : null;
  for (const [i, section] of sections.entries()) section.id = `year-${groups[i]!.year}`;
  mount(root, el("h1", {}, "My trips"), el("p", {}, `${trips.length} trip${trips.length === 1 ? "" : "s"}.`), ...(jump ? [jump] : []), ...sections);
}

/**
 * "Play on TV", the Chromecast moment: the phone posts a play request, every TV in the
 * household picks it up off its feed, and the phone watches `/presence` until one of them
 * reports the trip (and the cast photo) on screen.
 */
export function caster(api: Api, token: string, tripId: string, status: HTMLElement): (start?: string, label?: string) => Promise<void> {
  return async (start, label) => {
    status.textContent = `Asking your TV to play ${label ?? "this trip"}…`;
    let before: NowShowing[] = [];
    try {
      before = await api.presence(token).catch(() => []);
      await api.playTrip(token, tripId, start);
    } catch (e) {
      status.textContent = e instanceof Error ? e.message : String(e);
      return;
    }
    status.textContent = "Sent. Watching for the TV to switch…";
    const seen = await untilConfirmed(() => api.presence(token).then((now) => castConfirmation(now, before, start ? { trip: tripId, start } : { trip: tripId })));
    status.textContent = seen ?? "Sent. The TV switches when it next checks in.";
  };
}

function tripRow(api: Api, token: string, trip: { id: string; name: string; start: string; end: string; highlights: string[] }): HTMLElement {
  const open = el("button", { class: "link" }, "Open");
  const play = el("button", { class: "link" }, "Play on TV");
  const status = el("span", { class: "muted cast" });
  const cast = caster(api, token, trip.id, status);
  play.onclick = async () => {
    play.disabled = true;
    try {
      await cast(undefined, trip.name);
    } finally {
      play.disabled = false;
    }
  };
  const row = el("li", { class: "item trip" }, el("span", { class: "name" }, trip.name), el("span", { class: "state" }, tripSummaryLine(trip)), el("span", { class: "actions" }, play, " ", open), status);
  let detail: HTMLElement | null = null;
  open.onclick = async () => {
    if (detail) {
      detail.remove();
      detail = null;
      open.textContent = "Open";
      return;
    }
    open.disabled = true;
    try {
      const view = await api.trip(token, trip.id);
      detail = renderDetail(api, token, view, cast);
      row.append(detail);
      open.textContent = "Close";
    } catch (e) {
      row.append(el("span", { class: "error" }, e instanceof Error ? e.message : String(e)));
    } finally {
      open.disabled = false;
    }
  };
  return row;
}

function renderDetail(api: Api, token: string, trip: TripView, cast: (start?: string, label?: string) => Promise<void>): HTMLElement {
  const days = el("ol", { class: "days" });
  const lines = tripLines(trip);
  trip.days.forEach((d, i) => {
    const thumbs = el("div", { class: "thumbs" });
    for (const m of d.media.slice(0, 4)) {
      const img = el("img", { alt: "", loading: "lazy" });
      api
        .mediaUrl(token, m, "thumb")
        .then((r) => (img.src = r.url))
        .catch(() => img.remove());
      // Tap a photo: the TV opens the show at that photo.
      const toTv = el("button", { class: "link cast-photo", title: "Play on TV from this photo" }, "▶ TV");
      toTv.onclick = () => void cast(m, "this photo");
      thumbs.append(el("figure", {}, img, toTv));
    }
    const dayLabel = d.title ?? `Day ${d.index}`;
    const playDay = el("button", { class: "link" }, "Play this day on TV");
    playDay.disabled = d.media.length === 0;
    playDay.onclick = () => void cast(d.media[0], dayLabel);
    days.append(el("li", {}, el("p", {}, lines[i] ?? `Day ${d.index}`), thumbs, playDay));
  });
  return el("div", { class: "detail" }, ...(trip.narrative ? [el("p", { class: "story" }, trip.narrative.text)] : []), days);
}
