// store - the browser version's ~/.breadbin: downloaded files and catalogue
// responses live in the Cache API, small lists (play history, hidden games,
// settings) in localStorage. Every access is guarded, so a private window or
// blocked storage just means nothing is remembered.

const FILES = 'breadbin-files-v1';
const JSON_CACHE = 'breadbin-json-v1';

async function openCache(name) {
  try {
    return 'caches' in self ? await caches.open(name) : null;
  } catch {
    return null;
  }
}

/** GET `url` as bytes, keeping a permanent copy (game images, SIDs, ROMs). */
export async function cachedBytes(url, { onProgress } = {}) {
  const cache = await openCache(FILES);
  const hit = cache && (await cache.match(url));
  if (hit) return new Uint8Array(await hit.arrayBuffer());
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
  const bytes = await readWithProgress(res, onProgress);
  if (cache) {
    cache.put(url, new Response(bytes, { headers: { 'content-type': 'application/octet-stream' } })).catch(() => {});
  }
  return bytes;
}

async function readWithProgress(res, onProgress) {
  const total = Number(res.headers.get('content-length')) || 0;
  if (!onProgress || !res.body) return new Uint8Array(await res.arrayBuffer());
  const reader = res.body.getReader();
  const chunks = [];
  let got = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    got += value.length;
    onProgress(got, total);
  }
  const out = new Uint8Array(got);
  let o = 0;
  for (const c of chunks) {
    out.set(c, o);
    o += c.length;
  }
  return out;
}

/** GET `url` as text, reusing a cached copy younger than `maxAgeMs`. A stale
 *  copy is still served when the network fails. `fetcher` replaces the plain
 *  fetch, e.g. to go through the CSDb relay. */
export async function cachedText(url, maxAgeMs = 24 * 3600e3, fetcher = fetch) {
  const cache = await openCache(JSON_CACHE);
  const hit = cache && (await cache.match(url));
  const age = hit ? Date.now() - Number(hit.headers.get('x-fetched') || 0) : Infinity;
  if (hit && age < maxAgeMs) return hit.text();
  try {
    const res = await fetcher(url);
    if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
    const text = await res.text();
    if (cache) {
      cache.put(url, new Response(text, { headers: { 'x-fetched': String(Date.now()) } })).catch(() => {});
    }
    return text;
  } catch (e) {
    if (hit) return hit.text();
    throw e;
  }
}

export async function cachedJSON(url, maxAgeMs) {
  return JSON.parse(await cachedText(url, maxAgeMs));
}

/** Store or read a user-supplied file (their own ROMs) under a fixed key. */
export async function putBlob(key, bytes) {
  const cache = await openCache(FILES);
  if (!cache) return false;
  await cache.put(`/__breadbin/${key}`, new Response(bytes));
  return true;
}
export async function getBlob(key) {
  const cache = await openCache(FILES);
  const hit = cache && (await cache.match(`/__breadbin/${key}`));
  return hit ? new Uint8Array(await hit.arrayBuffer()) : null;
}
export async function deleteBlob(key) {
  const cache = await openCache(FILES);
  if (cache) await cache.delete(`/__breadbin/${key}`);
}

/** Bytes held in the download cache, for the settings panel. */
export async function cacheUsage() {
  try {
    const est = await navigator.storage?.estimate?.();
    return est?.usage ?? 0;
  } catch {
    return 0;
  }
}
export async function clearCaches() {
  try {
    await caches.delete(FILES);
    await caches.delete(JSON_CACHE);
  } catch {}
}

// ── small persistent lists ────────────────────────────────────────────────

export function load(key, fallback) {
  try {
    const v = localStorage.getItem(`breadbin.${key}`);
    return v == null ? fallback : JSON.parse(v);
  } catch {
    return fallback;
  }
}
export function save(key, value) {
  try {
    localStorage.setItem(`breadbin.${key}`, JSON.stringify(value));
  } catch {}
}

/** Most-recent-first list of {kind, id, title, cover}, capped. */
export function recordPlay(entry) {
  const list = load('played', []).filter((e) => !(e.kind === entry.kind && e.id === entry.id));
  list.unshift({ ...entry, at: Date.now() });
  save('played', list.slice(0, 40));
}
export function recentPlays(kind) {
  return load('played', []).filter((e) => e.kind === kind);
}
