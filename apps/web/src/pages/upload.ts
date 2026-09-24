// upload.<domain>/?t=<trip code> | ?h=<household code> | ?owner=1 | ?shared=<n>
// Join as a guest or with Amazon, pick photos, watch them go up, remove your own. The
// phone remembers the trip, so a return visit or a share-sheet launch skips the QR.

import { Api, ApiError } from "../api.js";
import { OWNER_TOKEN_KEY, beginSignIn, completeSignIn, savedTokens, takeOwnerHandoff } from "../auth.js";
import { loadConfig } from "../config.js";
import { familyCard } from "../family.js";
import { button, context, el, eyebrow, field, formatBytes, icon, link, mount, photo, topbar } from "../ui.js";
import { retryItem, runQueue, type QueueItem } from "../upload/queue.js";
import { REVIEW_PAGE, doneTitle, failures, inFlight, kept, latestSent, sendingTitle, shareHint, summaryLine, tally } from "../upload/view.js";
import { openSharedCache, sharedCount, takeSharedFiles } from "../upload/shared.js";
import { IndexedDbResumeStore } from "../upload/store.js";

const SESSION = "ml.upload.session";
const DEVICE = "ml.device";

/** A stable id for this browser; guest sessions are bound to it (F-153). */
export function deviceIdOf(storage: Storage): string {
  let id = storage.getItem(DEVICE);
  if (!id) {
    id = crypto.randomUUID();
    storage.setItem(DEVICE, id);
  }
  return id;
}

interface Session {
  token: string;
  displayName: string;
  code: string | null;
  /** The TV's household name, for a paired phone. */
  place?: string;
  /** False for a household code: it names no trip yet, so there is no trip page to open. */
  hasTrip?: boolean;
}

/** A page with nothing to do but say something: top bar, title, one line. */
function notice(root: HTMLElement, title: string, text: string): void {
  mount(root, topbar(), el("div", { class: "stack" }, el("h1", {}, title), el("p", { class: "lead" }, text)));
}

export async function main(root: HTMLElement, location: Location, storage: Storage): Promise<void> {
  const config = loadConfig();
  const api = new Api(config.apiBase, fetch, deviceIdOf(storage));
  const auth = { domain: config.cognitoDomain, clientId: config.cognitoClientId, redirectUri: `${location.origin}/callback` };
  const url = new URL(location.href);

  if (url.pathname === "/callback") {
    const { returnTo } = await completeSignIn(auth, storage, location.href);
    location.replace(returnTo);
    return;
  }

  // Owner session handed over from the link page in the fragment (US1).
  const handoff = takeOwnerHandoff(url.hash);
  if (handoff) {
    storage.setItem(OWNER_TOKEN_KEY, handoff);
    // This phone was just let in by a TV. A guest session saved from an earlier code
    // belongs to some other household and must not take these photos.
    storage.removeItem(SESSION);
    url.hash = "";
    globalThis.history?.replaceState(null, "", url.toString());
  }

  const code = url.searchParams.get("t") ?? url.searchParams.get("h");
  let session = readSession(storage, code);

  // Photos from the share sheet: upload them into the remembered trip straight away.
  const shared = sharedCount(url) > 0 ? await takeSharedFiles(await openSharedCache()) : [];
  if (shared.length > 0) {
    url.searchParams.delete("shared");
    globalThis.history?.replaceState(null, "", url.toString());
    if (session && (await sessionAlive(api, session.token))) {
      renderUploader(root, api, session, storage, shared);
      return;
    }
    storage.removeItem(SESSION);
    notice(root, "Scan the TV first", `${shared.length} photo${shared.length === 1 ? "" : "s"} arrived, but this phone is not in a trip yet. Scan the Invite code on the TV, then share again.`);
    return;
  }

  if (!session && (handoff || url.searchParams.get("owner") === "1")) {
    const token = storage.getItem(OWNER_TOKEN_KEY);
    if (token) {
      // A paired phone adds photos under the name it gave the TV.
      const me = await api.me(token).catch(() => null);
      session = { token, displayName: me?.display_name ?? "You", code: null, ...(me?.household_name ? { place: me.household_name } : {}) };
    }
  }

  if (!session && code) {
    const tokens = savedTokens(storage);
    if (tokens && url.searchParams.get("amazon") === "1") {
      const joined = await api.joinWithAmazon(code, tokens.idToken);
      session = { token: joined.token, displayName: joined.display_name, code, hasTrip: joined.trip !== null };
      storage.setItem(SESSION, JSON.stringify(session));
    }
  }

  if (!session) {
    if (!code) {
      notice(root, "Mantelpiece", "Scan the Invite code on the TV to add your photos.");
      return;
    }
    renderJoin(root, code, async (name) => {
      const joined = await api.joinAsGuest(code, name);
      session = { token: joined.token, displayName: joined.display_name, code, hasTrip: joined.trip !== null };
      storage.setItem(SESSION, JSON.stringify(session));
      renderUploader(root, api, session, storage);
    }, async () => {
      url.searchParams.set("amazon", "1");
      location.assign(await beginSignIn(auth, storage, url.toString()));
    });
    return;
  }

  renderUploader(root, api, session, storage);
}

function readSession(storage: Storage, code: string | null): Session | null {
  const raw = storage.getItem(SESSION);
  if (!raw) return null;
  const s = JSON.parse(raw) as Session;
  return code === null || s.code === code ? s : null;
}

/** Upload sessions last 30 days; a dead one means back to the QR, not a wall of failures. */
async function sessionAlive(api: Api, token: string): Promise<boolean> {
  try {
    await api.me(token);
    return true;
  } catch (e) {
    if (e instanceof ApiError && (e.status === 401 || e.status === 403)) return false;
    return true; // offline or a server hiccup: let the queue retry
  }
}

function renderJoin(root: HTMLElement, code: string, asGuest: (name: string) => Promise<void>, withAmazon: () => Promise<void>) {
  const name = el("input", { type: "text", placeholder: "Your name", autocomplete: "given-name", maxlength: "40" });
  const error = el("p", { class: "error" });
  const guest = button("Continue");
  guest.onclick = async () => {
    if (!name.value.trim()) {
      error.textContent = "Tell everyone who you are.";
      return;
    }
    guest.disabled = true;
    try {
      await asGuest(name.value.trim());
    } catch (e) {
      error.textContent = e instanceof Error ? e.message : String(e);
      guest.disabled = false;
    }
  };
  const amazon = button("Sign in with Amazon instead", { secondary: true, icon: "amazon" });
  amazon.onclick = () => void withAmazon();
  mount(
    root,
    topbar(),
    el(
      "div",
      { class: "stack" },
      eyebrow(`Trip code ${code}`),
      el("h1", {}, "Add your photos"),
      el("p", { class: "lead" }, "Your photos join everyone else's on the TV, credited to you. Nothing to install."),
    ),
    field("Your name, as the TV will show it", name),
    guest,
    amazon,
    error,
    el("p", { class: "small" }, "Guests can add and remove their own photos. Sign in with Amazon to be named on the trip for good."),
  );
}

/** A row for a file in flight or one that failed; everything else is counted, not listed. */
function stateText(it: QueueItem): { cls: string; text: string } {
  switch (it.progress.phase) {
    case "hashing":
      return { cls: "hashing", text: "Checking" };
    case "uploading":
      return { cls: "uploading", text: "" };
    case "done":
      return { cls: "done", text: "On the TV" };
    case "duplicate":
      return { cls: "duplicate", text: "Already on the TV" };
    case "failed":
      return { cls: "failed", text: it.error ?? "Did not send" };
  }
}

function renderUploader(root: HTMLElement, api: Api, session: Session, storage: Storage, pending: File[] = []) {
  const input = el("input", { type: "file", multiple: "", accept: "image/*,video/*", class: "file" });
  const store = new IndexedDbResumeStore();
  const chip = () => context(session.code ? `Trip ${session.code} · as ${session.displayName}` : `${session.place ?? "Your household"} · as ${session.displayName}`);
  const owner = Boolean(storage.getItem(OWNER_TOKEN_KEY));
  const family = () => familyCard(api, session.token, session.place ? { tvName: session.place } : {});

  // Object URLs for thumbnails, made only for files on screen; videos get a neutral panel.
  const previews = new Map<File, string>();
  const preview = (file: File, cls = ""): HTMLElement => {
    if (!file.type.startsWith("image/")) return el("div", { class: cls ? `thumb ${cls}` : "thumb" }, icon("video", 22));
    let u = previews.get(file);
    if (!u) {
      u = URL.createObjectURL(file);
      previews.set(file, u);
    }
    return photo(u, cls);
  };

  const picker = el(
    "label",
    { class: "picker" },
    el("span", { class: "circle" }, icon("images", 30)),
    el("span", { class: "big" }, "Choose photos"),
    input,
  );

  // A way out when this phone should join a different trip.
  const leave = link("Switch trip", { small: true });
  leave.onclick = () => {
    storage.removeItem(SESSION);
    notice(root, "Mantelpiece", "Scan the Invite code on the TV to join another trip.");
  };
  const sep = () => el("span", { class: "sep" }, "·");
  const footer = owner
    ? [
        el(
          "div",
          { class: "footer" },
          eyebrow("Your TV"),
          el(
            "div",
            { class: "row" },
            link("My trips", { icon: "memories", href: "/trips.html" }),
            sep(),
            link("People", { icon: "people", href: "/people.html" }),
            sep(),
            link("Ask", { icon: "ask", href: "/ask.html" }),
          ),
        ),
      ]
    : [];

  const hint = shareHint(globalThis.navigator?.userAgent ?? "");
  const showIdle = () =>
    mount(
      root,
      topbar({ right: chip() }),
      el(
        "div",
        { class: "stack" },
        el("h1", {}, "Add your photos"),
        el("p", { class: "lead" }, session.code ? "Photos and videos go straight to the TV and into this trip. Up to 2 GB each." : "Photos and videos go straight to the TV; trips are found from them. Up to 2 GB each."),
      ),
      picker,
      ...(hint ? [el("div", { class: "note" }, icon("share", 20), el("p", {}, hint))] : []),
      el("div", { class: "row end" }, leave),
      family(),
      ...footer,
    );

  // One batch at a time; `all` is its queue, `removed` what this phone took back.
  let all: readonly QueueItem[] = [];
  const removed = new Set<string>();

  // Rows for `items`, each failed one with Retry. A retry reports to `onRetry`, which
  // redraws only what it owns, so the rest of the page stays as it is.
  const retryRows = (items: readonly QueueItem[], onRetry: () => void) =>
    items.map((it) => {
      if (it.progress.phase !== "failed") return row(it);
      const retry = link("Retry", { icon: "retry", small: true });
      retry.onclick = async () => {
        retry.disabled = true;
        const index = all.indexOf(it);
        await retryItem(it, { api, bearer: session.token, store, onChange: onRetry }, { total: all.length, done: index + 1 });
        onRetry();
      };
      return row(it, retry);
    });
  const row = (it: QueueItem, action?: HTMLElement) => {
    const state = stateText(it);
    const progress =
      it.progress.phase === "uploading"
        ? el("div", { class: "bar" }, el("i", { style: `width: ${Math.round(it.progress.fraction * 100)}%` }))
        : el("span", { class: "state" }, state.text);
    return el(
      "li",
      { class: `item ${state.cls}` },
      preview(it.file, "small"),
      el("div", { class: "meta" }, el("div", { class: "top" }, el("span", { class: "name" }, it.file.name), el("span", { class: "size" }, formatBytes(it.file.size))), progress),
      ...(action ? [action] : []),
    );
  };

  // Sending: counts and a bar, the few files in flight, failures, the newest few sent.
  const title = el("h1");
  const overall = el("div", { class: "bar" }, el("i"));
  const summary = el("p", { class: "meta" });
  const active = el("ul", { class: "uploads" });
  const recent = el("div", { class: "recent" });
  let recentShown = -1;
  const showSending = () =>
    mount(
      root,
      topbar({ right: chip() }),
      el("div", { class: "stack" }, title, el("p", { class: "lead" }, "Keep this page open. If you leave, it picks up where it stopped when you come back.")),
      overall,
      summary,
      active,
      recent,
      input,
    );
  const renderSending = () => {
    const t = tally(all, removed);
    title.textContent = sendingTitle(t);
    (overall.firstElementChild as HTMLElement).style.width = `${Math.round(t.fraction * 100)}%`;
    const left = t.bytesLeft > 0 ? `${formatBytes(t.bytesLeft)} to go` : "Finishing up";
    const counts = summaryLine(t);
    summary.textContent = counts ? `${left} · ${counts}` : left;
    active.replaceChildren(...inFlight(all).map((it) => row(it)), ...retryRows(failures(all), renderSending));
    // The strip changes only when another photo lands, not on every progress tick.
    if (t.done !== recentShown) {
      recentShown = t.done;
      const latest = latestSent(all, 4);
      recent.replaceChildren(...(latest.length ? [eyebrow("On the TV now"), el("div", { class: "tiles" }, ...latest.map((it) => preview(it.file, "tile")))] : []));
    }
  };

  // Done: the outcome, what to do next, failures to retry, and the photos behind a fold.
  const showDone = () => {
    const t = tally(all, removed);
    const more = button("Add more photos", { icon: "images" });
    more.onclick = () => input.click();
    // A household code has no trip until the photos are organised; older saved sessions
    // (no flag) keep the link.
    const seeTrip = session.code && session.hasTrip !== false ? [el("a", { class: "button secondary", href: `/trip.html?t=${encodeURIComponent(session.code)}` }, icon("chev", 20), el("span", {}, "See the trip as it forms"))] : [];
    const heading = el("h1", {}, doneTitle(t));
    const counts = el("p", { class: "small" }, summaryLine(t));
    const recount = () => {
      const now = tally(all, removed);
      heading.textContent = doneTitle(now);
      counts.textContent = summaryLine(now);
    };
    // The ones that failed stay listed while retried, showing how the retry goes.
    const unsent = failures(all);
    const failed = el("ul", { class: "uploads" });
    const redrawFailed = () => {
      // Every retry landed: draw the screen again without the section, counting them in.
      if (unsent.every((it) => it.progress.phase === "done" || it.progress.phase === "duplicate")) {
        showDone();
        return;
      }
      failed.replaceChildren(...retryRows(unsent, redrawFailed));
      recount();
    };
    failed.replaceChildren(...retryRows(unsent, redrawFailed));
    mount(
      root,
      topbar({ right: chip() }),
      el(
        "div",
        { class: "finished" },
        el("div", { class: "check-circle" }, icon("check", 34)),
        heading,
        el("p", { class: "lead" }, "The trip takes shape over the next few minutes: days, a route and a story."),
        counts,
      ),
      ...(unsent.length > 0 ? [el("div", { class: "stack" }, eyebrow("Did not send"), failed)] : []),
      more,
      ...seeTrip,
      review(recount),
      family(),
      input,
    );
  };

  // "Review or remove": a grid of this batch's photos, a page at a time, each removable.
  const review = (changed: () => void): HTMLElement => {
    const mine = kept(all, removed);
    const grid = el("div", { class: "tiles review" });
    const label = el("span", {}, `Review or remove (${mine.length})`);
    const moreTiles = link("Show more", { small: true });
    let shown = 0;
    const tile = (it: QueueItem) => {
      const remove = el("button", { class: "remove", type: "button", "aria-label": `Remove ${it.file.name}` }, el("span", {}, icon("close", 16)));
      const box = el("div", { class: "tile-wrap" }, preview(it.file, "tile"), remove);
      remove.onclick = async () => {
        remove.disabled = true;
        try {
          await api.deleteMedia(session.token, it.mediaId!);
        } catch {
          remove.disabled = false;
          return;
        }
        removed.add(it.mediaId!);
        box.remove();
        label.textContent = `Review or remove (${kept(all, removed).length})`;
        changed();
      };
      return box;
    };
    const page = () => {
      const next = mine.slice(shown, shown + REVIEW_PAGE);
      shown += next.length;
      grid.append(...next.map(tile));
      moreTiles.hidden = shown >= mine.length;
    };
    moreTiles.onclick = page;
    const details = el(
      "details",
      { class: "review-box" },
      el("summary", {}, label, icon("chev", 18)),
      el("p", { class: "small" }, "Your photos stay yours. Take any of them off the TV, any time."),
      grid,
      moreTiles,
    );
    // Thumbnails load only once someone opens it.
    details.addEventListener("toggle", () => {
      if (details.open && shown === 0) page();
    });
    if (mine.length === 0) details.hidden = true;
    return details;
  };

  const upload = async (files: File[]) => {
    if (files.length === 0) return;
    input.disabled = true;
    all = [];
    removed.clear();
    recentShown = -1;
    active.replaceChildren();
    recent.replaceChildren();
    showSending();
    const onChange = (items: readonly QueueItem[]) => {
      if (items.length >= all.length) all = items;
      renderSending();
    };
    all = await runQueue(files, { api, bearer: session.token, store, onChange });
    showDone();
    input.disabled = false;
    input.value = "";
  };
  input.onchange = () => void upload(Array.from(input.files ?? []));

  showIdle();
  if (pending.length > 0) void upload(pending);
}
