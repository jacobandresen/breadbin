// ia - the Internet Archive: item metadata, files through its CORS endpoint,
// and GameBase64's collection (a zip of zips whose members the archive serves
// one by one). Everything is cached by store.js.

import { cachedBytes, cachedJSON } from './store.js';

const path = (p) => p.split('/').map(encodeURIComponent).join('/');

/** "Thunder (1990 12 01)(Forces of Evil)" -> "Thunder". */
export const displayTitle = (t) => t.replace(/\s*[([].*$/, '').replace(/\s+/g, ' ').trim() || t;

/** Item metadata (files, CSDb fields), cached for a month. */
export const metadata = (id) => cachedJSON(`https://archive.org/metadata/${encodeURIComponent(id)}`, 30 * 24 * 3600e3);

/** One file of an item, cached for good. */
export const download = (id, name, onProgress) => cachedBytes(`https://archive.org/cors/${encodeURIComponent(id)}/${path(name)}`, { onProgress });

/** A GameBase64 game zip ("b3/BUBBLEBO_01138_02.zip"), cached for good. */
export const gb64Game = (p, onProgress) => cachedBytes(`https://archive.org/cors/gb64v19/Games.zip/Games/${path(p)}`, { onProgress });

/** GameBase64's screenshot ("B/Bubble_Bobble.png"). */
export const gb64Shot = (p) => `https://archive.org/download/gb64v19/Screenshots.zip/Screenshots/${path(p)}`;

export const thumb = (id) => `https://archive.org/services/img/${encodeURIComponent(id)}`;
export const fileUrl = (id, name) => `https://archive.org/download/${encodeURIComponent(id)}/${path(name)}`;
export const itemUrl = (id) => `https://archive.org/details/${encodeURIComponent(id)}`;

/** A game's Internet Archive copy that already plays in the browser, if any. */
export const onlineAt = (g) => (g.online || g.ia) && itemUrl(g.online || g.ia);
