// Field notes on the phone (F-711): a highlight with its map laid over it, a switch back
// to the plain photo, and a way to share the composed picture. The map is a separate
// image the size of the photo, so two `contain` images in one box line up; the photo is
// never changed.

import { button, el, link } from "./ui.js";

export interface FieldNoteSummary {
  placement: { left: number; top: number; width: number; height: number };
  mode: string;
  certainty: "precise" | "approximate";
  credit: string;
}

/** Highlights that have a field note, in the trip's highlight order. */
export function withNotes(highlights: readonly string[], notes: Record<string, FieldNoteSummary> | undefined): string[] {
  return highlights.filter((m) => notes?.[m] !== undefined);
}

/** How a picture leaves the phone: the share sheet when it takes files, else a download. */
export function shareMethod(nav: { canShare?: (data: { files: File[] }) => boolean } | undefined, file: File): "share" | "download" {
  try {
    return nav?.canShare?.({ files: [file] }) ? "share" : "download";
  } catch {
    return "download";
  }
}

/** Fetch the composed picture and hand it to the share sheet, or download it. */
export async function shareFile(url: string, name: string, fetchFn: typeof fetch = fetch): Promise<"share" | "download"> {
  const blob = await (await fetchFn(url)).blob();
  const file = new File([blob], name, { type: "image/jpeg" });
  const nav = globalThis.navigator as Navigator | undefined;
  if (shareMethod(nav, file) === "share") {
    await nav!.share({ files: [file] });
    return "share";
  }
  const a = el("a", { href: URL.createObjectURL(blob), download: name });
  a.click();
  return "download";
}

/** A photo with its field note: switchable, and shareable when `share` is given. */
export function fieldNoteView(opts: {
  photo: Promise<string>;
  layer: Promise<string>;
  credit: string;
  /** Owner and co-editors: resolves the full-resolution composed picture's URL. */
  share?: () => Promise<string>;
  name?: string;
}): HTMLElement {
  const base = el("img", { alt: "", class: "fn-photo" });
  const over = el("img", { alt: "Map of where this was taken", class: "fn-layer" });
  void opts.photo.then((u) => (base.src = u)).catch(() => undefined);
  void opts.layer.then((u) => (over.src = u)).catch(() => over.remove());
  const stack = el("div", { class: "fn-stack" }, base, over);
  const toggle = link("Plain photo", { small: true });
  toggle.onclick = () => {
    over.hidden = !over.hidden;
    toggle.replaceChildren(over.hidden ? "Field note" : "Plain photo");
  };
  const status = el("p", { class: "small dim" });
  const actions = el("div", { class: "row" }, toggle);
  if (opts.share) {
    const share = button("Share", { icon: "share", secondary: true, inline: true });
    share.onclick = async () => {
      share.disabled = true;
      status.textContent = "Preparing the picture…";
      try {
        const how = await shareFile(await opts.share!(), opts.name ?? "field-note.jpg");
        status.textContent = how === "download" ? "Saved to your downloads." : "";
      } catch (e) {
        status.textContent = e instanceof Error && e.name === "AbortError" ? "" : "Could not prepare the picture. Try again.";
      } finally {
        share.disabled = false;
      }
    };
    actions.append(share);
  }
  return el("figure", { class: "fn" }, stack, el("figcaption", { class: "small dim" }, opts.credit), actions, status);
}
