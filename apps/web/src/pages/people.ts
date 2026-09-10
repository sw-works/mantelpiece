// upload.<domain>/people.html: People opt-in with consent copy, naming and merging, and
// My uploads with Remove (F-704, US9).

import { Api, type PersonView } from "../api.js";
import { OWNER_TOKEN_KEY } from "../auth.js";
import { loadConfig } from "../config.js";
import { CONSENT, mergeTargets, personLabel, sortPeople } from "../people.js";
import { el, mount } from "../ui.js";

export async function main(root: HTMLElement, _location: Location, storage: Storage): Promise<void> {
  const config = loadConfig();
  const api = new Api(config.apiBase);
  const token = storage.getItem(OWNER_TOKEN_KEY);
  if (!token) {
    mount(root, el("h1", {}, "People"), el("p", {}, "Sign in your TV first: scan the code on the TV with this phone."));
    return;
  }
  const render = async () => {
    const [state, uploads] = await Promise.all([api.people(token), api.myUploads(token).catch(() => [])]);
    const toggle = el("button", { class: `button ${state.people_enabled ? "secondary" : ""}` }, state.people_enabled ? CONSENT.turnOff : CONSENT.turnOn);
    toggle.onclick = async () => {
      toggle.disabled = true;
      if (state.people_enabled && !confirm("Delete all face data for this household?")) {
        toggle.disabled = false;
        return;
      }
      await api.setPeople(token, !state.people_enabled);
      await render();
    };
    const list = el("ul", { class: "uploads" });
    for (const p of sortPeople(state.people)) list.append(personRow(api, token, p, state.people, render));
    const mine = el("ul", { class: "uploads" });
    for (const u of uploads) {
      const row = el("li", { class: `item ${u.state}` }, el("span", { class: "name" }, u.filename), el("span", { class: "state" }, u.state));
      const remove = el("button", { class: "link" }, "Remove");
      remove.onclick = async () => {
        await api.deleteMedia(token, u.media_id);
        row.remove();
      };
      row.append(remove);
      mine.append(row);
    }
    mount(
      root,
      el("h1", {}, "People"),
      el("p", {}, state.people_enabled ? CONSENT.on : CONSENT.off),
      toggle,
      ...(state.people_enabled ? [el("h2", {}, "Who is who"), state.people.length ? list : el("p", {}, "Groups appear as trips are organised.")] : []),
      el("h2", {}, "My uploads"),
      uploads.length ? mine : el("p", {}, "Nothing uploaded from this phone yet."),
    );
  };
  await render();
}

function personRow(api: Api, token: string, p: PersonView, all: PersonView[], refresh: () => Promise<void>): HTMLLIElement {
  const name = el("input", { type: "text", placeholder: "Name", value: p.name ?? "", maxlength: "40" });
  const save = el("button", { class: "link" }, "Save");
  save.onclick = async () => {
    await api.updatePerson(token, p.id, { name: name.value });
    await refresh();
  };
  const merge = el("select", {});
  merge.append(el("option", { value: "" }, "Merge into…"));
  for (const t of mergeTargets(all, p.id)) merge.append(el("option", { value: t.id }, personLabel(t)));
  merge.onchange = async () => {
    if (!merge.value) return;
    await api.updatePerson(token, p.id, { merge_into: merge.value });
    await refresh();
  };
  return el("li", { class: "item" }, el("span", { class: "name" }, personLabel(p)), name, save, merge);
}
