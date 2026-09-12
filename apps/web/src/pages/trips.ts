// upload.<domain>/trips.html: every trip in the household for the owner's phone, grouped
// by year; open one to see its days and first photos. Needs the owner session from the
// link page, the same as People and Ask.

import { Api, type TripView } from "../api.js";
import { OWNER_TOKEN_KEY } from "../auth.js";
import { loadConfig } from "../config.js";
import { tripLines } from "./trip.js";
import { groupByYear, tripSummaryLine } from "../trips.js";
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

function tripRow(api: Api, token: string, trip: { id: string; name: string; start: string; end: string; highlights: string[] }): HTMLElement {
  const open = el("button", { class: "link" }, "Open");
  const row = el("li", { class: "item trip" }, el("span", { class: "name" }, trip.name), el("span", { class: "state" }, tripSummaryLine(trip)), open);
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
      detail = renderDetail(api, token, view);
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

function renderDetail(api: Api, token: string, trip: TripView): HTMLElement {
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
      thumbs.append(el("figure", {}, img));
    }
    days.append(el("li", {}, el("p", {}, lines[i] ?? `Day ${d.index}`), thumbs));
  });
  return el("div", { class: "detail" }, ...(trip.narrative ? [el("p", { class: "story" }, trip.narrative.text)] : []), days);
}
