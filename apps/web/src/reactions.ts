// Pure helpers for reactions on the read-only trip page (F-710).

import type { ReactionsView } from "./api.js";

export const VIEWER_NAME_KEY = "ml.viewer.name";

export function targetKey(target: { target: "day"; index: number } | { target: "media"; id: string }): string {
  return target.target === "day" ? `day:${target.index}` : `media:${target.id}`;
}

/** "❤ 3 · 1 comment" or "" when nothing yet. */
export function reactionLine(view: ReactionsView | null, key: string): string {
  const s = view?.summary[key];
  if (!s || (s.likes === 0 && s.comments === 0)) return "";
  const parts: string[] = [];
  if (s.likes) parts.push(`❤ ${s.likes}`);
  if (s.comments) parts.push(`${s.comments} comment${s.comments === 1 ? "" : "s"}`);
  return parts.join(" · ");
}

export function latestComment(view: ReactionsView | null, key: string): string | null {
  const c = view?.summary[key]?.latest_comment;
  return c ? `${c.by}: ${c.text}` : null;
}

/** A viewer's name is asked once and kept on the phone. */
export function viewerName(storage: Storage): string | null {
  const n = storage.getItem(VIEWER_NAME_KEY)?.trim();
  return n && n.length >= 2 ? n : null;
}
