import { main as link } from "./pages/link.js";
import { main as upload } from "./pages/upload.js";

const root = document.getElementById("app")!;
const page = document.body.dataset.page === "link" ? link : upload;
page(root, window.location, window.localStorage).catch((e: unknown) => {
  root.textContent = e instanceof Error ? e.message : String(e);
});

if ("serviceWorker" in navigator && import.meta.env.PROD) {
  navigator.serviceWorker.register("/sw.js").catch(() => undefined);
}
