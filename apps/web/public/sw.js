// App-shell cache so the uploader opens offline and resumes when the phone reconnects,
// and the Web Share Target receiver: photos shared from the phone's share sheet are parked
// in a cache and the uploader page picks them up.
const CACHE = "memorylane-shell-v2";
const SHARED = "memorylane-shared";
const SHARE_PATH = "/share-target";

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((c) => c.addAll(["/", "/index.html"])));
  self.skipWaiting();
});
self.addEventListener("activate", (event) => {
  event.waitUntil(caches.keys().then((keys) => Promise.all(keys.filter((k) => k !== CACHE && k !== SHARED).map((k) => caches.delete(k)))));
});

async function receiveShare(request) {
  const form = await request.formData();
  const files = form.getAll("media").filter((f) => typeof f !== "string");
  const cache = await caches.open(SHARED);
  await Promise.all((await cache.keys()).map((k) => cache.delete(k)));
  const stamp = Date.now();
  await Promise.all(
    files.map((file, i) =>
      cache.put(
        `/shared/${stamp}-${i}`,
        new Response(file, {
          headers: {
            "content-type": file.type || "application/octet-stream",
            "x-file-name": encodeURIComponent(file.name || `shared-${i}`),
            "x-file-modified": String(file.lastModified || stamp),
          },
        }),
      ),
    ),
  );
  return Response.redirect(`/?shared=${files.length}`, 303);
}

self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);
  if (url.origin !== self.location.origin) return;
  if (event.request.method === "POST" && url.pathname === SHARE_PATH) {
    event.respondWith(receiveShare(event.request));
    return;
  }
  if (event.request.method !== "GET") return;
  event.respondWith(caches.match(event.request).then((hit) => hit ?? fetch(event.request)));
});
