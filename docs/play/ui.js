// ui - shared page pieces: an element builder, toasts, the shelves of keycap
// cards and their cover art.

export const $ = (sel, el = document) => el.querySelector(sel);
/** Text for the pixel fonts, which have no accented letters ("Fjälldata" -> "Fjalldata"). */
export const pixel = (s) => s.normalize('NFD').replace(/[\u0300-\u036f]/g, '');
export const fmt = new Intl.NumberFormat('en');
export const CHIPS = ['var(--red)', 'var(--orange)', 'var(--yellow)', 'var(--green)', 'var(--blue)'];
let chipIndex = 0;
export const nextChip = () => CHIPS[chipIndex++ % CHIPS.length];

/** h('div.card', { onclick }, child, ...) */
export function h(tag, attrs = {}, ...children) {
  const [name, ...classes] = tag.split('.');
  const el = document.createElement(name || 'div');
  if (classes.length) el.className = classes.join(' ');
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v == null || v === false) continue;
    if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else if (k === 'style' && typeof v === 'object') Object.assign(el.style, v);
    else if (k in el) el[k] = v;
    else el.setAttribute(k, v);
  }
  el.append(...children.flat().filter((c) => c != null && c !== false));
  return el;
}

export function toast(msg, ms = 3500) {
  const t = $('#toast');
  t.textContent = msg;
  t.classList.add('show');
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => t.classList.remove('show'), ms);
}

/** The link that starts `id` from the `tab` shelf ('games', 'demos' or 'tunes') for whoever opens it. */
export const shareUrl = (tab, id) => `${location.origin}${location.pathname}#${tab}/${encodeURIComponent(id)}/play`;

/** Share the link to `id`: a phone's share sheet (Messages, WhatsApp...), or
 *  elsewhere the clipboard, saying so. */
export async function share(tab, id, title) {
  const url = shareUrl(tab, id);
  if (navigator.share && matchMedia('(pointer: coarse)').matches) {
    try {
      return await navigator.share({ title: `${title} on breadbin`, text: `Play ${title} on a C64 in your browser`, url });
    } catch (e) {
      if (e.name === 'AbortError') return; // the sheet was dismissed
    } // no sheet after all: copy instead
  }
  try {
    await navigator.clipboard.writeText(url);
  } catch {
    // no async clipboard (an http page other than localhost, an old browser): the old way
    const ta = h('textarea', { value: url, readOnly: true, style: { position: 'fixed', opacity: '0' } });
    document.body.append(ta);
    ta.select();
    const ok = document.execCommand?.('copy');
    ta.remove();
    if (!ok) return prompt('Copy this link:', url);
  }
  toast(`🔗 Link to ${title} copied: paste it to a friend.`);
}

export const shareButton = (tab, item) =>
  h('button.key.pill.share', { onclick: () => share(tab, item.id, item.title), title: 'Share a link that starts it' }, '🔗 Share');

export const closeButton = (dlg) => h('button.key.icon.small.close', { onclick: () => dlg.close(), 'aria-label': 'Close' }, '✕');

/** A definition list filled with add(label, value); empty values are skipped. */
export function facts() {
  const el = h('dl.facts');
  el.add = (k, v) => v && el.append(h('dt', {}, k), h('dd', {}, v));
  return el;
}

/** Cover art that fades in, falls back down a list of URLs, then to text. */
export function art(urls, label, { boxart = false, badge, tag, wip } = {}) {
  const wrap = h(`div.art${boxart ? '.boxart' : ''}`);
  const img = h('img', { alt: '', loading: 'lazy', decoding: 'async', referrerPolicy: 'no-referrer' });
  const list = urls.filter(Boolean);
  let i = 0;
  const next = () => {
    if (i < list.length) return (img.src = list[i++]);
    img.remove();
    wrap.prepend(h('div.placeholder', {}, label));
  };
  // archive.org answers missing thumbnails with a tiny generic icon
  img.addEventListener('load', () => (img.naturalWidth < 40 ? next() : img.classList.add('loaded')));
  img.addEventListener('error', next);
  next();
  wrap.append(img);
  if (badge) wrap.append(h('span.badge', {}, badge));
  if (tag) wrap.append(h('span.tag', {}, tag));
  if (wip) wrap.append(h('span.wip', { title: `Work in progress: ${wip}` }, 'WIP'));
  return wrap;
}

/** The end of a shelf page: what breadbin runs on, and the way to its credits. */
export function footer() {
  return h('footer.foot', {},
    h('img', { src: '../images/breadbin.svg', alt: '', width: 20, height: 20 }),
    h('span', {}, 'breadbin bakes on free software: the MEGA65 Open ROMs, Pascual DOS, vc64web and the scene\'s own archives. '),
    h('button.linkish', { onclick: () => document.querySelector('#open-settings')?.click() }, 'Credits'));
}

/** What a playtest failure (tools/playtest's reason) means for a visitor. */
const WIP_WORDS = [
  [/joystick moved nothing/, 'it starts, but the playtest couldn\'t yet show the joystick moving the player'],
  [/CPU jammed/, 'it crashed in the playtest'],
  [/stayed at BASIC|blank screen/, 'it didn\'t start in the playtest'],
  [/picture never changed/, 'its picture stood still in the playtest'],
  [/no sound/, 'no sound came out in the playtest'],
  [/ERROR|BRK AT/, 'it stopped with an error in the playtest'],
];

/** For a title that failed its playtest (`item.wip` is why): it may not start or play right. */
export function wipNote(item) {
  if (!item.wip) return null;
  const why = WIP_WORDS.find(([re]) => re.test(item.wip))?.[1] || `the playtest said: ${item.wip.trim()}`;
  return h('p.needs.wip-note', { title: item.wip.trim() }, h('b', {}, '🚧 Work in progress. '), `Breadbin is still working on this one: ${why}. It may not start or play right.`);
}

/**
 * A titled row of cards over `items` that renders when scrolled into view and
 * can expand into a grid ("Show All"), or (`grid`) is one from the start.
 * Returns { el, ready } or null if empty.
 */
export function shelf(parent, title, items, card, { wide = false, size = 24, grid = false } = {}) {
  if (!items.length) return null;
  const toggle = h('button.key.pill', { hidden: grid || items.length <= 8 });
  const body = h(grid ? 'div.grid' : 'div.row');
  const more = h('div.more', { hidden: true }, h('button.key.pill', { onclick: () => show(shown + size) }, 'More'));
  const el = h(`section.shelf${wide ? '.wide' : ''}`, {},
    h('header.shelf-head', {}, h('span.shelf-chip', { style: { background: nextChip() } }), h('h2.shelf-title', {}, pixel(title)), h('span.shelf-rule'), toggle),
    body, more);
  parent.append(el);
  for (let i = 0; i < 8; i++) body.append(h('div.card.skeleton', {}, h('div.art'), h('div.card-title')));

  let shown = 0;
  let expanded = grid;
  const label = () => (toggle.innerHTML = expanded ? 'Show Less' : `Show All<span class="n"> (${fmt.format(items.length)})</span>`);
  const show = (n) => {
    if (!shown) body.replaceChildren();
    body.append(...items.slice(shown, n).map(card));
    shown = Math.min(n, items.length);
    more.hidden = !expanded || shown >= items.length;
  };
  label();
  toggle.addEventListener('click', () => {
    expanded = !expanded;
    body.className = expanded ? 'grid' : 'row';
    label();
    if (expanded && shown < size * 2) show(size * 2);
    more.hidden = !expanded || shown >= items.length;
    if (!expanded) el.scrollIntoView({ block: 'nearest' });
  });

  const ready = new Promise((resolve) => {
    const io = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      io.disconnect();
      show(size);
      resolve(el);
    }, { rootMargin: '400px 0px' });
    io.observe(el);
  });
  return { el, ready, expand: () => toggle.click() };
}
