// upload.<domain>/people.html: People opt-in with consent copy, naming and merging, and
// My uploads with Remove (F-704, US9).

import { Api, type PersonView } from "../api.js";
import { OWNER_TOKEN_KEY } from "../auth.js";
import { loadConfig } from "../config.js";
import { CONSENT, mergeTargets, personLabel, sortPeople } from "../people.js";
import { button, el, eyebrow, link, mount, photo, topbar } from "../ui.js";

const BACK = "/?owner=1";

export async function main(root: HTMLElement, _location: Location, storage: Storage): Promise<void> {
  const config = loadConfig();
  const api = new Api(config.apiBase);
  const token = storage.getItem(OWNER_TOKEN_KEY);
  if (!token) {
    mount(root, topbar({ back: BACK }), el("div", { class: "stack" }, el("h1", {}, "People"), el("p", { class: "lead" }, "Sign in your TV first: scan the code on the TV with this phone.")));
    return;
  }
  const render = async () => {
    const [state, uploads] = await Promise.all([api.people(token), api.myUploads(token).catch(() => [])]);
    const toggle = button(state.people_enabled ? CONSENT.turnOff : CONSENT.turnOn, { secondary: state.people_enabled });
    toggle.onclick = async () => {
      toggle.disabled = true;
      if (state.people_enabled && !confirm("Delete all face data for this household?")) {
        toggle.disabled = false;
        return;
      }
      await api.setPeople(token, !state.people_enabled);
      await render();
    };
    const list = el("ul", { class: "people" });
    for (const p of sortPeople(state.people)) list.append(personRow(api, token, p, state.people, render));
    const mine = el("ul", { class: "mine" });
    for (const u of uploads) {
      const remove = link("Remove");
      const row = el(
        "li",
        {},
        photo(api.mediaUrl(token, u.media_id, "thumb").then((r) => r.url), "small"),
        el("div", { class: "meta" }, el("span", { class: "name" }, u.filename), el("p", { class: "small" }, u.state)),
        remove,
      );
      remove.onclick = async () => {
        await api.deleteMedia(token, u.media_id);
        row.remove();
      };
      mine.append(row);
    }
    mount(
      root,
      topbar({ back: BACK }),
      el("div", { class: "stack" }, el("h1", {}, "People"), el("p", { class: "lead" }, state.people_enabled ? CONSENT.on : CONSENT.off)),
      toggle,
      ...(state.people_enabled ? [el("div", { class: "stack" }, eyebrow("Who is who"), state.people.length ? list : el("p", {}, "Groups appear as trips are organised."))] : []),
      el("div", { class: "stack" }, eyebrow("My uploads"), uploads.length ? mine : el("p", {}, "Nothing uploaded from this phone yet.")),
    );
  };
  await render();
}

function personRow(api: Api, token: string, p: PersonView, all: PersonView[], refresh: () => Promise<void>): HTMLLIElement {
  const name = el("input", { type: "text", placeholder: "Name", value: p.name ?? "", maxlength: "40", "aria-label": personLabel(p) });
  const save = link("Save", { small: true });
  save.onclick = async () => {
    await api.updatePerson(token, p.id, { name: name.value });
    await refresh();
  };
  const merge = el("select", { class: "link-select", "aria-label": "Merge into" });
  merge.append(el("option", { value: "" }, "Merge into…"));
  for (const t of mergeTargets(all, p.id)) merge.append(el("option", { value: t.id }, personLabel(t)));
  merge.onchange = async () => {
    if (!merge.value) return;
    await api.updatePerson(token, p.id, { merge_into: merge.value });
    await refresh();
  };
  const sample = p.sample_media[0];
  return el(
    "li",
    { class: "person" },
    photo(sample ? api.mediaUrl(token, sample, "thumb").then((r) => r.url) : Promise.reject(new Error("no sample")), "round"),
    el("div", { class: "grow" }, name),
    el("div", { class: "side" }, el("p", { class: "small" }, `${p.media_count} photo${p.media_count === 1 ? "" : "s"}`), save, merge),
  );
}
