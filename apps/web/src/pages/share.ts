// upload.<domain>/s.html?s=<share id>: an album someone shared by link (F-601). Read-only,
// no sign-in; the link stops working after seven days or when the owner revokes it.

import { Api, ApiError, type SharedAlbum } from "../api.js";
import { loadConfig } from "../config.js";
import { el, mount } from "../ui.js";

/** "Shared by Hanyu's living room · 12 photos · until 16 Sep 2026". */
export function shareLine(album: SharedAlbum, now: Date = new Date()): string {
  const parts: string[] = [];
  if (album.shared_by) parts.push(`Shared by ${album.shared_by}`);
  parts.push(`${album.items.length} photo${album.items.length === 1 ? "" : "s"}`);
  if (album.expires_at) {
    const until = new Date(album.expires_at);
    const days = Math.ceil((until.getTime() - now.getTime()) / 86_400_000);
    parts.push(days <= 1 ? "expires today" : `${days} days left`);
  }
  return parts.join(" · ");
}

/** What the page says for a link that no longer opens. */
export function closedMessage(e: unknown): string {
  if (e instanceof ApiError && e.status === 410) return "This link has expired. Ask whoever shared it to send a new one.";
  if (e instanceof ApiError && e.status === 404) return "This link does not open an album.";
  return e instanceof Error ? e.message : String(e);
}

export async function main(root: HTMLElement, location: Location): Promise<void> {
  const config = loadConfig();
  const api = new Api(config.apiBase);
  const id = new URL(location.href).searchParams.get("s");
  if (!id) {
    mount(root, el("h1", {}, "MemoryLane"), el("p", {}, "Open the link you were sent to see the album."));
    return;
  }
  mount(root, el("h1", {}, "Opening the album…"));
  let album: SharedAlbum;
  try {
    album = await api.sharedAlbum(id);
  } catch (e) {
    mount(root, el("h1", {}, "Nothing to show"), el("p", {}, closedMessage(e)));
    return;
  }
  const grid = el("div", { class: "grid" });
  for (const item of album.items) {
    const caption = [item.landmark, item.taken_at?.slice(0, 10)].filter(Boolean).join(" · ");
    grid.append(el("figure", {}, el("img", { src: item.url, alt: item.landmark ?? "" , loading: "lazy" }), el("figcaption", {}, caption)));
  }
  mount(root, el("h1", {}, album.name), el("p", {}, shareLine(album)), album.items.length ? grid : el("p", {}, "Every photo in this album has been taken down."));
}
