// upload.<domain>/ask.html: hold to talk, or type, and the answer shows on the TV (F-405).

import { Api } from "../api.js";
import { describeOutcome, nowShowingLine, speechRecognition, transcriptOf } from "../ask.js";
import { OWNER_TOKEN_KEY } from "../auth.js";
import { loadConfig } from "../config.js";
import { button, el, field, icon, mount, photo, topbar } from "../ui.js";

const BACK = "/?owner=1";

export async function main(root: HTMLElement, _location: Location, storage: Storage): Promise<void> {
  const config = loadConfig();
  const api = new Api(config.apiBase);
  const token = storage.getItem(OWNER_TOKEN_KEY);
  const heading = el("div", { class: "stack tight" }, el("h1", {}, "Ask about your photos"), el("p", { class: "lead" }, "Ask out loud or type. The answer shows on the TV, over whatever is on screen."));
  if (!token) {
    mount(root, topbar({ back: BACK }), heading, el("p", {}, "Sign in your TV first: scan the code on the TV with this phone."));
    return;
  }

  // "Now on the TV": what the freshest TV reports, with its current photo when there is one.
  const nowTitle = el("span", { class: "title" }, "Now on your TV");
  const status = el("p", { class: "small" }, "Checking what is on the TV…");
  const nowBox = el("div", { class: "thumb wide" }, icon("tv", 20));
  const now = el("div", { class: "card compact" }, nowBox, el("div", { class: "titles" }, nowTitle, status));

  const answer = el("p", { class: "answer" });
  const answerLabel = el("span", { class: "label small" }, "MemoryLane");
  const also = el("p", { class: "small ok" });
  const answerBlock = el("div", { class: "field" }, answerLabel, answer, also);
  answerBlock.hidden = true;
  const input = el("input", { type: "text", placeholder: "What's that statue in the background?" });
  const send = button("Ask", { secondary: true });
  const Recognition = speechRecognition(window as unknown as Record<string, unknown>);
  const caption = el("span", {}, Recognition ? "Hold to talk" : "Voice needs Safari or Chrome; type instead");
  const talk = el("button", { class: "mic", type: "button" }, el("span", { class: "circle" }, icon("mic", 48)), caption);
  talk.disabled = !Recognition;

  let tv: string | undefined;
  api
    .presence(token)
    .then((list) => {
      const freshest = [...list].sort((a, b) => b.at.localeCompare(a.at))[0];
      tv = freshest?.device_id;
      status.textContent = nowShowingLine(freshest);
      if (freshest) now.replaceChild(photo(api.mediaUrl(token, freshest.media, "thumb").then((r) => r.url), "wide"), nowBox);
    })
    .catch(() => (status.textContent = nowShowingLine(undefined)));

  const ask = async (question: string) => {
    if (!question.trim()) return;
    send.disabled = true;
    answerBlock.hidden = false;
    answer.textContent = "Asking…";
    also.textContent = "";
    try {
      const result = await api.ask(token, question.trim(), tv);
      answer.textContent = describeOutcome(result);
      also.textContent = result.mode === "describe" ? "Also on the TV" : result.mode === "search" ? "Playing on the TV" : "";
    } catch (e) {
      answer.textContent = e instanceof Error ? e.message : String(e);
    } finally {
      send.disabled = false;
    }
  };
  send.onclick = () => void ask(input.value);
  input.onkeydown = (e) => {
    if (e.key === "Enter") void ask(input.value);
  };
  if (Recognition) {
    let active: InstanceType<typeof Recognition> | null = null;
    const start = () => {
      active = new Recognition();
      active.lang = navigator.language || "en-US";
      active.interimResults = false;
      active.maxAlternatives = 1;
      active.onresult = (event) => {
        input.value = transcriptOf(event);
        void ask(input.value);
      };
      active.onerror = (event) => {
        answerBlock.hidden = false;
        answer.textContent = `Could not hear you (${event.error}).`;
      };
      active.onend = () => (active = null);
      active.start();
      talk.classList.add("listening");
      caption.textContent = "Listening… release to ask";
    };
    const stop = () => {
      active?.stop();
      talk.classList.remove("listening");
      caption.textContent = "Hold to talk";
    };
    talk.onpointerdown = start;
    talk.onpointerup = stop;
    talk.onpointercancel = stop;
  }
  mount(root, topbar({ back: BACK }), heading, now, el("div", { class: "mic-wrap" }, talk), field("Your question", input, true), send, answerBlock);
}
