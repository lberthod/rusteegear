// Service worker PhysioTech.ch (repris de Mouvéo `public/sw.js`, adapté).
// Réseau d'abord + cache de repli pour tout ce qui vient de cette origine
// (page, moteur `pkg/` ~25 Mo mis en cache à la première lecture, manifeste,
// clips de la voix de Christophe) ; cache d'abord pour MediaPipe (wasm + modèles, CDN) — après une
// première séance avec caméra, tout tourne hors ligne.
const CACHE = "physiotech-v9";
const CORE = ["./reeduc.html", "./manifest.json", "./favicon.svg", ...["ready", "position", "calibrated", "three", "two", "one", "start", "arm_start", "good", "good_control", "mission_complete", "next_game", "reposition", "paused", "resume", "trunk", "symmetry", "slow_return"].map((c) => `./voice/christophe/${c}.mp3`)];
const MEDIAPIPE = /(cdn\.jsdelivr\.net\/npm\/@mediapipe|storage\.googleapis\.com\/mediapipe-models)/;

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((cache) => cache.addAll(CORE)).then(() => self.skipWaiting()));
});
self.addEventListener("activate", (event) => {
  event.waitUntil(caches.keys().then((keys) => Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key)))).then(() => self.clients.claim()));
});
self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (MEDIAPIPE.test(url.href)) {
    // Modèles et wasm MediaPipe : lourds et versionnés, cache d'abord.
    event.respondWith(caches.match(request).then((cached) => cached || fetch(request).then((response) => {
      if (response.ok || response.type === "opaque") { const copy = response.clone(); caches.open(CACHE).then((cache) => cache.put(request, copy)); }
      return response;
    })));
    return;
  }
  if (url.origin !== self.location.origin) return;
  event.respondWith(fetch(request).then((response) => {
    if (response.ok) { const copy = response.clone(); caches.open(CACHE).then((cache) => cache.put(request, copy)); }
    return response;
  }).catch(() => caches.match(request).then((cached) => cached || caches.match("./reeduc.html"))));
});
