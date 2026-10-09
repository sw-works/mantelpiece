// upload.<domain>/trips.html: every trip in the household for the owner's phone, grouped
// by year; open one to see its days and first photos. Needs the owner session from the
// link page, the same as People and Ask.

import { Api, type TripView } from "../api.js";
import { OWNER_TOKEN_KEY } from "../auth.js";
import { loadConfig } from "../config.js";
import { tripLines } from "./trip.js";
import { castConfirmation, groupByYear, type NowShowing, tripSummaryLine, untilConfirmed } from "../trips.js";
import { fieldNoteView, withNotes } from "../fieldnote.js";
import { button, el, icon, link, mount, photo, topbar } from "../ui.js";

const BACK = "/?owner=1";

function header(...rest: (Node | string)[]): (Node | string)[] {
  return [topbar({ back: BACK }), el("div", { class: "stack tight" }, el("h1", {}, "My trips"), ...rest)];
}

export async function main(root: HTMLElement, _location: Location, storage: Storage): Promise<void> {
  const config = loadConfig();
  const api = new Api(config.apiBase);
  const token = storage.getItem(OWNER_TOKEN_KEY);
  if (!token) {
    mount(root, ...header(el("p", { class: "lead" }, "Sign in your TV first: scan the code on the TV with this phone.")));
    return;
  }
  mount(root, ...header(el("p", { class: "lead" }, "Loading…")));
  const trips = await api.trips(token);
  if (trips.length === 0) {
    mount(root, ...header(el("p", { class: "lead" }, "No trips yet. Press Add photos on the TV and scan the code.")));
    return;
  }
  const groups = groupByYear(trips);
  const sections: HTMLElement[] = [];
  for (const group of groups) {
    const list = el("ul", { class: "trips" });
    for (const trip of group.trips) list.append(tripRow(api, token, trip));
    sections.push(el("section", { class: "year" }, ...(groups.length > 1 ? [el("h2", {}, group.year)] : []), list));
  }
  for (const [i, section] of sections.entries()) section.id = `year-${groups[i]!.year}`;
  // The year row: the newest year with its count, then the others as jump links.
  const first = groups[0]!;
  const years = el(
    "p",
    { class: "years" },
    el("span", { class: "current" }, `${first.year} · ${first.trips.length} trip${first.trips.length === 1 ? "" : "s"}`),
    ...groups.slice(1).map((g) => link(g.year, { href: `#year-${g.year}` })),
  );
  mount(root, ...header(el("p", { class: "lead" }, "Tap Play on TV to send a trip to the living room; tap a photo to start there.")), years, ...sections);
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
  const open = button("Open", { secondary: true, inline: true });
  const play = button("Play on TV", { icon: "tv", inline: true });
  const status = el("p", { class: "cast" });
  const cast = caster(api, token, trip.id, status);
  // The line turns green once a TV reports the trip on screen.
  const castAndMark = async (start?: string, label?: string) => {
    status.classList.remove("ok");
    await cast(start, label);
    status.classList.toggle("ok", (status.textContent ?? "").startsWith("Now showing"));
  };
  play.onclick = async () => {
    play.disabled = true;
    try {
      await castAndMark(undefined, trip.name);
    } finally {
      play.disabled = false;
    }
  };
  const hero = trip.highlights[0];
  const row = el(
    "li",
    { class: "tripcard" },
    el(
      "div",
      { class: "head" },
      photo(hero ? api.mediaUrl(token, hero, "thumb").then((r) => r.url) : Promise.reject(new Error("no hero")), "hero"),
      el("div", { class: "titles" }, el("span", { class: "title" }, trip.name), el("p", { class: "small" }, tripSummaryLine(trip))),
      icon("chev", 20),
    ),
    el("div", { class: "actions" }, play, open),
    status,
  );
  let detail: HTMLElement | null = null;
  open.onclick = async () => {
    if (detail) {
      detail.remove();
      detail = null;
      open.replaceChildren("Open");
      return;
    }
    open.disabled = true;
    try {
      const view = await api.trip(token, trip.id);
      detail = renderDetail(api, token, view, castAndMark);
      row.append(detail);
      open.replaceChildren("Close");
    } catch (e) {
      row.append(el("p", { class: "error small" }, e instanceof Error ? e.message : String(e)));
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
    const thumbs = el("div", { class: "grid4" });
    for (const m of d.media.slice(0, 4)) {
      // Tap Play under a photo: the TV opens the show at that photo.
      const toTv = link("Play", { icon: "tv", small: true });
      toTv.setAttribute("title", "Play on TV from this photo");
      toTv.onclick = () => void cast(m, "this photo");
      thumbs.append(el("figure", {}, photo(api.mediaUrl(token, m, "thumb").then((r) => r.url), "tile"), toTv));
    }
    const dayLabel = d.title ?? `Day ${d.index}`;
    const playDay = link("Play this day on TV", { icon: "tv" });
    playDay.disabled = d.media.length === 0;
    playDay.onclick = () => void cast(d.media[0], dayLabel);
    days.append(el("li", {}, el("p", { class: "meta fg" }, lines[i] ?? `Day ${d.index}`), thumbs, el("div", { class: "row" }, playDay)));
  });
  // F-711: the highlights that have a field note, each viewable and shareable.
  const noted = withNotes(trip.highlights, trip.field_notes);
  const notes: HTMLElement[] = noted.length
    ? [
        el(
          "section",
          { class: "stack tight" },
          el("p", { class: "meta fg" }, `Field notes · ${noted.length}`),
          ...noted.slice(0, 6).map((m) =>
            fieldNoteView({
              photo: api.mediaUrl(token, m, "tv").then((r) => r.url),
              layer: api.mediaUrl(token, m, "fieldnote").then((r) => r.url),
              credit: trip.field_notes![m]!.credit,
              share: () => api.fieldNoteFull(token, m).then((r) => r.url),
              name: `${trip.name} field note.jpg`,
            }),
          ),
        ),
      ]
    : [];
  return el("div", { class: "detail" }, ...(trip.narrative ? [el("p", { class: "story" }, trip.narrative.text)] : []), days, ...notes);
}
