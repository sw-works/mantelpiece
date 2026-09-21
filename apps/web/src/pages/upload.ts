// upload.<domain>/?t=<trip code> | ?h=<household code> | ?owner=1 | ?shared=<n>
// Join as a guest or with Amazon, pick photos, watch them go up, remove your own. The
// phone remembers the trip, so a return visit or a share-sheet launch skips the QR.

import { Api, ApiError } from "../api.js";
import { OWNER_TOKEN_KEY, beginSignIn, completeSignIn, savedTokens, takeOwnerHandoff } from "../auth.js";
import { loadConfig } from "../config.js";
import { button, context, el, eyebrow, field, formatBytes, icon, link, mount, photo, topbar } from "../ui.js";
import { retryItem, runQueue, type QueueItem } from "../upload/queue.js";
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
      notice(root, "MemoryLane", "Scan the Invite code on the TV to add your photos.");
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

/** What the queue adds up to, for the title, the overall bar and the summary line. */
function tally(items: readonly QueueItem[], removed: ReadonlySet<string>) {
  let done = 0;
  let duplicate = 0;
  let failed = 0;
  let fraction = 0;
  let bytesLeft = 0;
  for (const it of items) {
    const phase = it.progress.phase;
    if (phase === "done") done++;
    else if (phase === "duplicate") duplicate++;
    else if (phase === "failed") failed++;
    const f = phase === "uploading" ? it.progress.fraction : phase === "hashing" ? 0 : 1;
    fraction += f;
    bytesLeft += Math.round(it.file.size * (1 - f));
  }
  const total = items.length;
  const removedCount = items.filter((it) => it.mediaId && removed.has(it.mediaId)).length;
  return { total, done, duplicate, failed, sent: done + duplicate, fraction: total ? fraction / total : 0, bytesLeft, removed: removedCount };
}

function stateOf(it: QueueItem): { cls: string; text: string } {
  switch (it.progress.phase) {
    case "hashing":
      return it.progress.fraction === 0 ? { cls: "waiting", text: "Waiting" } : { cls: "hashing", text: "Checking" };
    case "uploading":
      return { cls: "uploading", text: "" };
    case "done":
      return { cls: "done", text: "On the TV" };
    case "duplicate":
      return { cls: "duplicate", text: "Already on the TV" };
    case "failed":
      return { cls: "failed", text: `Failed · ${it.error ?? "unknown error"}` };
  }
}

function renderUploader(root: HTMLElement, api: Api, session: Session, storage: Storage, pending: File[] = []) {
  const input = el("input", { type: "file", multiple: "", accept: "image/*,video/*", class: "file" });
  const list = el("ul", { class: "uploads" });
  const store = new IndexedDbResumeStore();
  const chip = () => context(session.code ? `Trip ${session.code} · as ${session.displayName}` : `${session.place ?? "Your household"} · as ${session.displayName}`);
  const owner = Boolean(storage.getItem(OWNER_TOKEN_KEY));

  // Object URLs for the row thumbnails, one per picked file; videos get a neutral panel.
  const previews = new Map<File, string>();
  const preview = (file: File): HTMLElement => {
    if (!file.type.startsWith("image/")) return el("div", { class: "thumb" }, icon("video", 22));
    let u = previews.get(file);
    if (!u) {
      u = URL.createObjectURL(file);
      previews.set(file, u);
    }
    return photo(u);
  };

  const picker = el(
    "label",
    { class: "picker" },
    el("span", { class: "circle" }, icon("images", 30)),
    el("span", { class: "big" }, "Choose from your camera roll"),
    el("p", { class: "small" }, "Photos and videos, up to 2 GB each"),
    input,
  );

  // Where the photos go, and a way out when this phone should join a different trip.
  const leave = link("Switch trip");
  leave.onclick = () => {
    storage.removeItem(SESSION);
    notice(root, "MemoryLane", "Scan the Invite code on the TV to join another trip.");
  };
  const where = el(
    "div",
    { class: "row" },
    el("p", { class: "meta grow" }, session.code ? `Adding to trip ${session.code}.` : "Adding to your household. Trips are found from the photos."),
    leave,
  );
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

  const showIdle = () =>
    mount(
      root,
      topbar({ right: chip() }),
      el("div", { class: "stack" }, el("h1", {}, "Pick photos"), el("p", { class: "lead" }, "Choose a trip's worth. Big batches are fine; uploads resume if you lose signal.")),
      picker,
      el("div", { class: "note" }, icon("share", 20), el("p", {}, "Or share straight from your camera roll: on Android, add this page to your home screen and MemoryLane appears in the share sheet.")),
      where,
      ...footer,
    );

  const title = el("h1");
  const overall = el("div", { class: "bar" }, el("i"));
  const summary = el("p", { class: "meta" });
  const showUploading = () =>
    mount(
      root,
      topbar({ right: chip() }),
      el("div", { class: "stack" }, title, el("p", { class: "lead" }, "Keep this page open, or come back later: it picks up where it stopped. The TV is already showing them arrive.")),
      overall,
      summary,
      list,
      input,
    );

  const showDone = (t: ReturnType<typeof tally>) => {
    const more = button("Add more photos", { icon: "images" });
    more.onclick = () => input.click();
    // A household code has no trip until the photos are organised; older saved sessions
    // (no flag) keep the link.
    const seeTrip = session.code && session.hasTrip !== false ? [el("a", { class: "button secondary", href: `/trip.html?t=${encodeURIComponent(session.code)}` }, icon("chev", 20), el("span", {}, "See the trip as it forms"))] : [];
    const heading = t.failed > 0 ? `${t.sent} of ${t.total} on the TV` : t.sent === 1 ? "Your photo is on the TV" : `All ${t.sent} on the TV`;
    const parts = [`${t.total} chosen`, `${t.duplicate} already on the TV`, `${t.removed} removed by you`];
    if (t.failed > 0) parts.push(`${t.failed} failed`);
    mount(
      root,
      topbar({ right: chip() }),
      el(
        "div",
        { class: "finished" },
        el("div", { class: "check-circle" }, icon("check", 34)),
        el("h1", {}, heading),
        el("p", { class: "lead" }, "The trip takes shape over the next few minutes: days, a route and a story."),
      ),
      el("p", { class: "small" }, parts.join(" · ")),
      more,
      ...seeTrip,
      list,
      el("p", { class: "small" }, "Your photos stay yours. Remove any of them from the list above, any time."),
      input,
    );
  };

  const upload = async (files: File[]) => {
    if (files.length === 0) return;
    input.disabled = true;
    let all: readonly QueueItem[] = [];
    const removed = new Set<string>();
    showUploading();
    const render = (items: readonly QueueItem[]) => {
      if (items.length >= all.length) all = items;
      const t = tally(all, removed);
      title.textContent = `${t.sent} of ${t.total} sent`;
      (overall.firstElementChild as HTMLElement).style.width = `${Math.round(t.fraction * 100)}%`;
      summary.textContent = t.bytesLeft > 0 ? `${formatBytes(t.bytesLeft)} to go` : "Finishing up";
      list.replaceChildren(
        ...all
          .filter((it) => !(it.mediaId && removed.has(it.mediaId)))
          .map((it, index) => {
            const state = stateOf(it);
            const progress =
              it.progress.phase === "uploading"
                ? el("div", { class: "bar" }, el("i", { style: `width: ${Math.round(it.progress.fraction * 100)}%` }))
                : el("span", { class: "state" }, state.text);
            const row = el(
              "li",
              { class: `item ${state.cls}` },
              preview(it.file),
              el("div", { class: "meta" }, el("div", { class: "top" }, el("span", { class: "name" }, it.file.name), el("span", { class: "size" }, formatBytes(it.file.size))), progress),
            );
            if (it.mediaId) {
              const remove = link("Remove");
              remove.onclick = async () => {
                await api.deleteMedia(session.token, it.mediaId!);
                removed.add(it.mediaId!);
                row.remove();
              };
              row.append(remove);
            }
            if (it.progress.phase === "failed") {
              const retry = link("Retry", { icon: "retry" });
              retry.onclick = async () => {
                retry.disabled = true;
                await retryItem(it, { api, bearer: session.token, store, onChange: () => render(all) }, { total: all.length, done: index + 1 });
                render(all);
              };
              row.append(retry);
            }
            return row;
          }),
      );
    };
    const items = await runQueue(files, { api, bearer: session.token, store, onChange: render });
    render(items);
    showDone(tally(items, removed));
    input.disabled = false;
    input.value = "";
  };
  input.onchange = () => void upload(Array.from(input.files ?? []));

  showIdle();
  if (pending.length > 0) void upload(pending);
}
