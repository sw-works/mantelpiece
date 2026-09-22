// Family channel on the phone: hear back from the TV. The TV loves a batch of photos and
// this phone gets a notification, or at least a line in its inbox. Web push needs a
// service worker and, on iPhone, the page added to the Home Screen first (iOS 16.4+).

import type { Api, InboxNote } from "./api.js";
import { OWNER_TOKEN_KEY } from "./auth.js";
import { el, eyebrow, icon, link } from "./ui.js";

/** What this browser can do about notifications. */
export type PushSupport = "ready" | "install" | "denied" | "unsupported";

export interface PushEnv {
  hasServiceWorker: boolean;
  hasPushManager: boolean;
  /** `Notification.permission`, or null when there is no Notification API. */
  permission: NotificationPermission | null;
  userAgent: string;
  /** Opened from the Home Screen (`display-mode: standalone` or `navigator.standalone`). */
  standalone: boolean;
}

export function pushSupport(env: PushEnv): PushSupport {
  const ios = /iPhone|iPad|iPod/.test(env.userAgent);
  // Safari on iOS only exposes push to Home Screen web apps.
  if (ios && !env.standalone) return "install";
  if (!env.hasServiceWorker || !env.hasPushManager || env.permission === null) return "unsupported";
  if (env.permission === "denied") return "denied";
  return "ready";
}

export function currentPushEnv(): PushEnv {
  const nav = navigator as Navigator & { standalone?: boolean };
  return {
    hasServiceWorker: "serviceWorker" in navigator,
    hasPushManager: typeof window !== "undefined" && "PushManager" in window,
    permission: typeof Notification === "undefined" ? null : Notification.permission,
    userAgent: navigator.userAgent,
    standalone: Boolean(nav.standalone) || (typeof matchMedia === "function" && matchMedia("(display-mode: standalone)").matches),
  };
}

/** The VAPID key as the Push API wants it. */
export function base64UrlToBytes(value: string): Uint8Array<ArrayBuffer> {
  const padded = value.replace(/-/g, "+").replace(/_/g, "/") + "=".repeat((4 - (value.length % 4)) % 4);
  const raw = atob(padded);
  const out = new Uint8Array(new ArrayBuffer(raw.length));
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
  return out;
}

/** Ask permission, subscribe with the API's key, hand the subscription to the API. */
export async function turnOnPush(api: Api, bearer: string): Promise<boolean> {
  const permission = await Notification.requestPermission();
  if (permission !== "granted") return false;
  const registration = await navigator.serviceWorker.ready;
  const { key } = await api.pushKey();
  const existing = await registration.pushManager.getSubscription();
  const subscription = existing ?? (await registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: base64UrlToBytes(key) }));
  await api.subscribePush(bearer, subscription.toJSON());
  return true;
}

/** "2 min ago", "3 h ago", "yesterday". */
export function ago(at: string, now: number): string {
  const seconds = Math.max(0, Math.round((now - Date.parse(at)) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  return days === 1 ? "yesterday" : `${days} days ago`;
}

/** One inbox line: "Grandma's TV loved your 14 photos". */
export function describeNote(note: InboxNote): string {
  const noun = note.count === 1 ? "photo" : "photos";
  if (note.kind === "love") return `${note.from} loved your ${note.count} ${noun}`;
  return `${note.from} reacted to your ${note.count} ${noun}`;
}

/** The session token behind this phone: a paired phone's, or a guest's. */
export function phoneToken(storage: Storage): string | null {
  const owner = storage.getItem(OWNER_TOKEN_KEY);
  if (owner) return owner;
  const raw = storage.getItem("ml.upload.session");
  if (!raw) return null;
  try {
    return (JSON.parse(raw) as { token?: string }).token ?? null;
  } catch {
    return null;
  }
}

/**
 * The card under the uploader: what the TV said, a switch for notifications and the way to
 * go live. Fills itself in once the inbox arrives; stays quiet when the API is unreachable.
 */
export function familyCard(api: Api, bearer: string, opts: { tvName?: string; env?: PushEnv; now?: () => number } = {}): HTMLElement {
  const list = el("ul", { class: "inbox" });
  const status = el("p", { class: "small" });
  const card = el(
    "section",
    { class: "family stack tight" },
    eyebrow("From the TV"),
    list,
    status,
    el("a", { class: "button secondary", href: "/live.html" }, icon("video", 20), el("span", {}, `Go live on ${opts.tvName ?? "the TV"}`)),
  );
  const now = opts.now ?? Date.now;

  const renderPush = (on: boolean) => {
    status.replaceChildren();
    if (on) {
      status.textContent = "Notifications are on: you will hear when they react.";
      return;
    }
    const support = pushSupport(opts.env ?? currentPushEnv());
    if (support === "install") {
      status.textContent = "To hear when they react, add this page to your Home Screen (Share, Add to Home Screen), then open it from there.";
    } else if (support === "denied") {
      status.textContent = "Notifications are blocked for this page in the browser settings.";
    } else if (support === "unsupported") {
      status.textContent = "This browser cannot show notifications; reactions still appear here.";
    } else {
      const turnOn = link("Tell me when they react", { icon: "heart" });
      turnOn.onclick = async () => {
        turnOn.disabled = true;
        const ok = await turnOnPush(api, bearer).catch(() => false);
        renderPush(ok);
        if (!ok) turnOn.disabled = false;
      };
      status.append(turnOn);
    }
  };

  void api
    .inbox(bearer)
    .then(({ notes, push }) => {
      list.replaceChildren(
        ...(notes.length === 0
          ? [el("li", { class: "small dim" }, "Nothing yet. When the TV loves your photos, it shows up here.")]
          : notes.slice(0, 5).map((n) => el("li", { class: "row" }, el("span", { class: "heart" }, icon("heart", 18)), el("span", { class: "grow" }, describeNote(n)), el("span", { class: "small dim" }, ago(n.at, now()))))),
      );
      renderPush(push);
    })
    .catch(() => card.remove());
  return card;
}
