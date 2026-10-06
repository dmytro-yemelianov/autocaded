// AutoCADED offline support. Network first, cache as fallback: a new deploy
// is picked up on the next load, and the last copy that loaded keeps working
// offline. Install precaches the app shell and every asset assets/manifest.json
// names (font, screen menu, sample drawings).
const CACHE = 'autocaded-v2';
const SHELL = [
  './',
  './index.html',
  './input.mjs',
  './manifest.webmanifest',
  './icon.svg',
  './icon-192.png',
  './icon-512.png',
  './icon-maskable-512.png',
  './apple-touch-icon.png',
  './pkg/acad_wasm.js',
  './pkg/acad_wasm_bg.wasm',
  './assets/manifest.json',
];

async function precache() {
  const cache = await caches.open(CACHE);
  // One by one: a missing optional file must not abort the install.
  const add = (url) => cache.add(new Request(url, { cache: 'reload' })).catch(() => {});
  await Promise.all(SHELL.map(add));
  try {
    const manifest = await (await fetch('./assets/manifest.json', { cache: 'no-cache' })).json();
    const files = [manifest.font, manifest.menu, ...(manifest.samples || []).map((s) => s.file)];
    await Promise.all(files.filter(Boolean).map((file) => add(`./assets/${file}`)));
  } catch (err) {
    // No staged assets: the engine still runs with an empty drawing.
  }
}

self.addEventListener('install', (event) => {
  event.waitUntil(precache().then(() => self.skipWaiting()));
});

self.addEventListener('activate', (event) => {
  event.waitUntil((async () => {
    for (const key of await caches.keys()) {
      if (key !== CACHE) await caches.delete(key);
    }
    await self.clients.claim();
  })());
});

self.addEventListener('fetch', (event) => {
  const request = event.request;
  if (request.method !== 'GET' || new URL(request.url).origin !== self.location.origin) return;
  event.respondWith((async () => {
    const cache = await caches.open(CACHE);
    try {
      // HTTP's own fresh cache can otherwise serve an older JS/wasm pair
      // even though this service worker promises network-first updates.
      const response = await fetch(request, { cache: 'no-cache' });
      if (response.ok) cache.put(request, response.clone());
      return response;
    } catch (err) {
      const cached = await cache.match(request, { ignoreSearch: true });
      if (cached) return cached;
      if (request.mode === 'navigate') {
        const shell = await cache.match('./');
        if (shell) return shell;
      }
      throw err;
    }
  })());
});
