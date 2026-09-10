import { main as link } from "./pages/link.js";
import { main as trip } from "./pages/trip.js";
import { main as upload } from "./pages/upload.js";

const root = document.getElementById("app")!;
const pages = { link, trip, upload } as const;
const name = document.body.dataset.page;
const page = name === "link" || name === "trip" ? pages[name] : upload;
page(root, window.location, window.localStorage).catch((e: unknown) => {
  root.textContent = e instanceof Error ? e.message : String(e);
});

if ("serviceWorker" in navigator && import.meta.env.PROD) {
  navigator.serviceWorker.register("/sw.js").catch(() => undefined);
}
