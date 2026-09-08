// App-shell cache so the uploader opens offline and resumes when the phone reconnects.
const CACHE = "memorylane-shell-v1";
self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((c) => c.addAll(["/", "/index.html"])));
});
self.addEventListener("fetch", (event) => {
  if (event.request.method !== "GET" || new URL(event.request.url).origin !== self.location.origin) return;
  event.respondWith(caches.match(event.request).then((hit) => hit ?? fetch(event.request)));
});
