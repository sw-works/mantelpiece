import { main as ask } from "./pages/ask.js";
import { main as link } from "./pages/link.js";
import { main as live } from "./pages/live.js";
import { main as people } from "./pages/people.js";
import { main as share } from "./pages/share.js";
import { main as trip } from "./pages/trip.js";
import { main as trips } from "./pages/trips.js";
import { main as upload } from "./pages/upload.js";

const root = document.getElementById("app")!;
const pages = { ask, link, live, people, share, trip, trips, upload } as const;
const name = document.body.dataset.page;
const page = name === "link" || name === "live" || name === "trip" || name === "trips" || name === "ask" || name === "people" || name === "share" ? pages[name] : upload;
page(root, window.location, window.localStorage).catch((e: unknown) => {
  root.textContent = e instanceof Error ? e.message : String(e);
});

if ("serviceWorker" in navigator && import.meta.env.PROD) {
  navigator.serviceWorker.register("/sw.js").catch(() => undefined);
}
