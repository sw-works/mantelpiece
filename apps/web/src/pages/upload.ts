// upload.<domain>/?t=<trip code> | ?h=<household code> | ?owner=1 | ?shared=<n>
// Join as a guest or with Amazon, pick photos, watch them go up, remove your own. The
// phone remembers the trip, so a return visit or a share-sheet launch skips the QR.

import { Api, ApiError } from "../api.js";
import { OWNER_TOKEN_KEY, beginSignIn, completeSignIn, savedTokens, takeOwnerHandoff } from "../auth.js";
import { loadConfig } from "../config.js";
import { el, formatBytes, mount } from "../ui.js";
import { labelFor, retryItem, runQueue, type QueueItem } from "../upload/queue.js";
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
    mount(
      root,
      el("h1", {}, "Scan the TV first"),
      el("p", {}, `${shared.length} photo${shared.length === 1 ? "" : "s"} arrived, but this phone is not in a trip yet. Scan the Invite code on the TV, then share again.`),
    );
    return;
  }

  if (!session && (handoff || url.searchParams.get("owner") === "1")) {
    const token = storage.getItem(OWNER_TOKEN_KEY);
    if (token) session = { token, displayName: "You", code: null };
  }

  if (!session && code) {
    const tokens = savedTokens(storage);
    if (tokens && url.searchParams.get("amazon") === "1") {
      const joined = await api.joinWithAmazon(code, tokens.idToken);
      session = { token: joined.token, displayName: joined.display_name, code };
      storage.setItem(SESSION, JSON.stringify(session));
    }
  }

  if (!session) {
    if (!code) {
      mount(root, el("h1", {}, "MemoryLane"), el("p", {}, "Scan the Invite code on the TV to add your photos."));
      return;
    }
    renderJoin(root, code, async (name) => {
      const joined = await api.joinAsGuest(code, name);
      session = { token: joined.token, displayName: joined.display_name, code };
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
  const guest = el("button", { class: "button" }, "Continue");
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
  const amazon = el("button", { class: "button secondary" }, "Sign in with Amazon instead");
  amazon.onclick = () => void withAmazon();
  mount(
    root,
    el("h1", {}, "Add your photos"),
    el("p", {}, `Trip code ${code}. Your photos join everyone else's on the TV, credited to you.`),
    name,
    guest,
    amazon,
    error,
  );
}

function renderUploader(root: HTMLElement, api: Api, session: Session, storage: Storage, pending: File[] = []) {
  const input = el("input", { type: "file", multiple: "", accept: "image/*,video/*" });
  const list = el("ul", { class: "uploads" });
  const status = el("p", { class: "status" }, `Uploading as ${session.displayName}.`);
  const store = new IndexedDbResumeStore();

  const upload = async (files: File[]) => {
    if (files.length === 0) return;
    input.disabled = true;
    status.textContent = `Uploading ${files.length} file${files.length === 1 ? "" : "s"}…`;
    let all: readonly QueueItem[] = [];
    const render = (items: readonly QueueItem[]) => {
      if (items.length >= all.length) all = items;
      list.replaceChildren(
        ...all.map((it, index) => {
          const row = el("li", { class: `item ${it.progress.phase}` }, el("span", { class: "name" }, it.file.name), el("span", { class: "size" }, formatBytes(it.file.size)), el("span", { class: "state" }, labelFor(it)));
          if (it.mediaId) {
            const remove = el("button", { class: "link" }, "Remove");
            remove.onclick = async () => {
              await api.deleteMedia(session.token, it.mediaId!);
              row.remove();
            };
            row.append(remove);
          }
          if (it.progress.phase === "failed") {
            const retry = el("button", { class: "link" }, "Retry");
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
    const ok = items.filter((i) => i.mediaId).length;
    status.textContent = `${ok} of ${items.length} uploaded. The TV is building the memory.`;
    input.disabled = false;
    input.value = "";
  };
  input.onchange = () => void upload(Array.from(input.files ?? []));

  // Where the photos go, and a way out when this phone should join a different trip.
  const where = el("span", {}, session.code ? `Adding to trip ${session.code}.` : "Adding to your household.");
  const leave = el("button", { class: "link" }, "Switch trip");
  leave.onclick = () => {
    storage.removeItem(SESSION);
    mount(root, el("h1", {}, "MemoryLane"), el("p", {}, "Scan the Invite code on the TV to join another trip."));
  };

  const mine = storage.getItem(OWNER_TOKEN_KEY) ? [el("p", {}, el("a", { href: "/trips.html" }, "My trips"), " · ", el("a", { href: "/people.html" }, "People"), " · ", el("a", { href: "/ask.html" }, "Ask"))] : [];
  mount(
    root,
    el("h1", {}, pending.length > 0 ? "Sending your photos" : "Pick photos"),
    status,
    el("p", { class: "where" }, where, " ", leave),
    ...mine,
    el("label", { class: "picker" }, "Choose from your camera roll", input),
    el("p", { class: "hint" }, "On Android, add this page to your home screen and MemoryLane shows up in the share sheet."),
    list,
  );
  if (pending.length > 0) void upload(pending);
}
