// ia - the Internet Archive as breadbin's catalogue. Games and demos come from
// its C64 software library: one small search per shelf, ranked by how often each
// item is downloaded, and the disk images themselves through archive.org's CORS
// endpoint. Responses are cached (see store.js), so a revisit is instant.

import { cachedBytes, cachedJSON } from './store.js';

const SEARCH = 'https://archive.org/advancedsearch.php';
const META = 'https://archive.org/metadata/';
const CORS = 'https://archive.org/cors/';

export const GAMES = 'softwarelibrary_c64_games';
export const DEMOS = 'softwarelibrary_c64_demos';

const FIELDS = ['identifier', 'title', 'downloads', 'date', 'creator', 'mobygames_genre', 'csdb_group', 'csdb_release_date', 'csdb'];

/** Search the archive. Returns { total, docs }. */
export async function search(q, { rows = 24, page = 1, sort = 'downloads desc', maxAgeMs } = {}) {
  const params = new URLSearchParams({ q, rows: String(rows), page: String(page), output: 'json' });
  for (const f of FIELDS) params.append('fl[]', f);
  if (sort) params.append('sort[]', sort);
  const json = await cachedJSON(`${SEARCH}?${params}`, maxAgeMs);
  return { total: json.response.numFound, docs: json.response.docs.map(cleanDoc) };
}

function first(v) {
  return Array.isArray(v) ? v[0] : v;
}

/** Normalise a search doc into the shape the shelves use. */
function cleanDoc(d) {
  const title = String(first(d.title) || d.identifier).trim();
  const group = String(first(d.csdb_group) || first(d.creator) || '').replace(/,\s*$/, '').trim();
  const yearMatch = String(first(d.csdb_release_date) || first(d.date) || title).match(/\b(19[7-9]\d|20[0-4]\d)\b/);
  return {
    id: d.identifier,
    title: displayTitle(title),
    rawTitle: title,
    group,
    genre: first(d.mobygames_genre) || '',
    downloads: Number(d.downloads) || 0,
    year: yearMatch ? Number(yearMatch[1]) : 0,
    csdb: first(d.csdb) || '',
  };
}

/** "Thunder (1990 12 01)(Forces of Evil)" -> "Thunder". */
export function displayTitle(t) {
  return t.replace(/\s*[([].*$/, '').replace(/\s+/g, ' ').trim() || t;
}

/** Full item metadata (files list, CSDb fields), cached for a month. */
export async function metadata(id) {
  return cachedJSON(`${META}${encodeURIComponent(id)}`, 30 * 24 * 3600e3);
}

/** Download one file of an item through the CORS endpoint, cached for good. */
export function download(id, name, onProgress) {
  const path = name.split('/').map(encodeURIComponent).join('/');
  return cachedBytes(`${CORS}${encodeURIComponent(id)}/${path}`, { onProgress });
}

export function thumb(id) {
  return `https://archive.org/services/img/${encodeURIComponent(id)}`;
}

export function fileUrl(id, name) {
  return `https://archive.org/download/${encodeURIComponent(id)}/${name.split('/').map(encodeURIComponent).join('/')}`;
}

export function itemUrl(id) {
  return `https://archive.org/details/${encodeURIComponent(id)}`;
}

/** Quote a value for an archive search query. */
export function q(value) {
  return `"${String(value).replace(/["\\]/g, '')}"`;
}

// GameBase64's collection, as a zip of zips on the Internet Archive; single
// members are served (with CORS) by the archive's zip viewer.
const GB64 = 'https://archive.org/cors/gb64v19/';

/** A GameBase64 game zip ("b3/BUBBLEBO_01138_02.zip"), cached for good. */
export function gb64Game(path, onProgress) {
  return cachedBytes(`${GB64}Games.zip/Games/${path.split('/').map(encodeURIComponent).join('/')}`, { onProgress });
}

/** GameBase64's screenshot ("B/Bubble_Bobble.png") as an image URL. */
export function gb64Shot(path) {
  return `https://archive.org/download/gb64v19/Screenshots.zip/Screenshots/${path.split('/').map(encodeURIComponent).join('/')}`;
}
