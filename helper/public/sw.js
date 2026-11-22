// Keeps the page itself (not your scenes) on the iPad, so it opens without a
// connection. Scenes are always fetched fresh, or come from the drafts the
// page keeps (see app.js).
//
// index.php registers this with the page's files in its address
// (sw.js?v=…&app=app.js?v=…&css=style.css?v=…), each carrying its file's
// modification time: an upload changes the address, so a new copy of this
// worker installs and replaces the old copies. Nothing to bump by hand.
const params = new URL(self.location).searchParams;
const APP = params.get('app') || 'app.js';
const CSS = params.get('css') || 'style.css';
const VERSION = `writer-${params.get('v')}-${APP}-${CSS}`;
const SHELL = ['./', APP, CSS];

self.addEventListener('install', event => {
  event.waitUntil(caches.open(VERSION).then(cache => cache.addAll(SHELL)).then(() => self.skipWaiting()));
});

self.addEventListener('activate', event => {
  event.waitUntil(
    caches.keys().then(keys => Promise.all(keys.filter(k => k !== VERSION).map(k => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener('fetch', event => {
  const url = new URL(event.request.url);
  // The API is never cached: it's live data, or nothing.
  if (event.request.method !== 'GET' || url.pathname.endsWith('api.php')) return;
  // The network when there is one (so updates arrive), else the copy; any
  // page address offline gets the page.
  event.respondWith(
    fetch(event.request)
      .then(response => {
        if (response.ok) {
          const copy = response.clone();
          caches.open(VERSION).then(cache => cache.put(event.request, copy));
        }
        return response;
      })
      .catch(() => caches.match(event.request).then(hit => hit || caches.match('./'))),
  );
});
