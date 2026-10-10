// Trip sketch maps on the phone (F-712): the hand-drawn map of the whole trip at the top of
// the trip, its map-data credit beneath, and Share for the owner and co-editors.

import { button, el } from "./ui.js";
import { shareFile } from "./fieldnote.js";

/** The trip map, or nothing when it has not been drawn (the request fails with 404). */
export function sketchMapView(opts: {
  map: Promise<{ url: string; credit: string }>;
  /** Owner and co-editors: share the picture. */
  share?: boolean;
  name?: string;
}): HTMLElement {
  const img = el("img", { alt: "Hand-drawn map of the trip", class: "sketch-map" });
  const credit = el("p", { class: "small dim" });
  const status = el("p", { class: "small dim" });
  const figure = el("figure", { class: "sketch" }, img, credit);
  figure.hidden = true;
  void opts.map
    .then((m) => {
      img.src = m.url;
      credit.textContent = m.credit;
      figure.hidden = false;
      if (opts.share) {
        const share = button("Share map", { icon: "share", secondary: true, inline: true });
        share.onclick = async () => {
          share.disabled = true;
          try {
            const how = await shareFile(m.url, opts.name ?? "trip-map.png", fetch, "image/png");
            status.textContent = how === "download" ? "Saved to your downloads." : "";
          } catch (e) {
            status.textContent = e instanceof Error && e.name === "AbortError" ? "" : "Could not share the map. Try again.";
          } finally {
            share.disabled = false;
          }
        };
        figure.append(el("div", { class: "row" }, share), status);
      }
    })
    .catch(() => figure.remove());
  return figure;
}
