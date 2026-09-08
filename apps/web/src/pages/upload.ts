// upload.<domain>/?t=<trip code> | ?h=<household code> | ?owner=1
// Join as a guest or with Amazon, pick photos, watch them go up, remove your own.

import { Api } from "../api.js";
import { beginSignIn, completeSignIn, savedTokens } from "../auth.js";
import { loadConfig } from "../config.js";
import { el, formatBytes, mount } from "../ui.js";
import { runQueue, type QueueItem } from "../upload/queue.js";
import { IndexedDbResumeStore } from "../upload/store.js";

const SESSION = "ml.upload.session";
const OWNER_TOKEN = "ml.owner.token";

interface Session {
  token: string;
  displayName: string;
  code: string | null;
}

export async function main(root: HTMLElement, location: Location, storage: Storage): Promise<void> {
  const config = loadConfig();
  const api = new Api(config.apiBase);
  const auth = { domain: config.cognitoDomain, clientId: config.cognitoClientId, redirectUri: `${location.origin}/callback` };
  const url = new URL(location.href);

  if (url.pathname === "/callback") {
    const { returnTo } = await completeSignIn(auth, storage, location.href);
    location.replace(returnTo);
    return;
  }

  const code = url.searchParams.get("t") ?? url.searchParams.get("h");
  let session = readSession(storage, code);

  if (!session && url.searchParams.get("owner") === "1") {
    const token = storage.getItem(OWNER_TOKEN);
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
      renderUploader(root, api, session);
    }, async () => {
      url.searchParams.set("amazon", "1");
      location.assign(await beginSignIn(auth, storage, url.toString()));
    });
    return;
  }

  renderUploader(root, api, session);
}

function readSession(storage: Storage, code: string | null): Session | null {
  const raw = storage.getItem(SESSION);
  if (!raw) return null;
  const s = JSON.parse(raw) as Session;
  return code === null || s.code === code ? s : null;
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

function renderUploader(root: HTMLElement, api: Api, session: Session) {
  const input = el("input", { type: "file", multiple: "", accept: "image/*,video/*" });
  const list = el("ul", { class: "uploads" });
  const status = el("p", { class: "status" }, `Uploading as ${session.displayName}.`);
  const store = new IndexedDbResumeStore();

  input.onchange = async () => {
    const files = Array.from(input.files ?? []);
    if (files.length === 0) return;
    input.disabled = true;
    status.textContent = `Uploading ${files.length} file${files.length === 1 ? "" : "s"}…`;
    const render = (items: readonly QueueItem[]) => {
      list.replaceChildren(
        ...items.map((it) => {
          const pct = Math.round(it.progress.fraction * 100);
          const label = it.error ?? `${it.progress.phase} ${pct}%`;
          const row = el("li", { class: `item ${it.progress.phase}` }, el("span", { class: "name" }, it.file.name), el("span", { class: "size" }, formatBytes(it.file.size)), el("span", { class: "state" }, label));
          if (it.mediaId) {
            const remove = el("button", { class: "link" }, "Remove");
            remove.onclick = async () => {
              await api.deleteMedia(session.token, it.mediaId!);
              row.remove();
            };
            row.append(remove);
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

  mount(
    root,
    el("h1", {}, "Pick photos"),
    status,
    el("label", { class: "picker" }, "Choose from your camera roll", input),
    list,
  );
}
