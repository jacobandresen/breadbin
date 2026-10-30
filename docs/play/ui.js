// ui - shared page pieces: an element builder, toasts, the shelves of keycap
// cards and their cover art.

export const $ = (sel, el = document) => el.querySelector(sel);
/** Text for the Sixtyfour pixel font, which has no accented letters ("Fjälldata" -> "Fjalldata"). */
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

export const closeButton = (dlg) => h('button.key.icon.small.close', { onclick: () => dlg.close(), 'aria-label': 'Close' }, '✕');

/** A definition list filled with add(label, value); empty values are skipped. */
export function facts() {
  const el = h('dl.facts');
  el.add = (k, v) => v && el.append(h('dt', {}, k), h('dd', {}, v));
  return el;
}

/** Cover art that fades in, falls back down a list of URLs, then to text. */
export function art(urls, label, { boxart = false, badge } = {}) {
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
  return wrap;
}

/**
 * A titled row of cards over `items` that renders when scrolled into view and
 * can expand into a grid ("Show All"). Returns { el, ready } or null if empty.
 */
export function shelf(parent, title, items, card, { wide = false, size = 24 } = {}) {
  if (!items.length) return null;
  const toggle = h('button.key.pill', { hidden: items.length <= 8 });
  const body = h('div.row');
  const more = h('div.more', { hidden: true }, h('button.key.pill', { onclick: () => show(shown + size) }, 'More'));
  const el = h(`section.shelf${wide ? '.wide' : ''}`, {},
    h('header.shelf-head', {}, h('span.shelf-chip', { style: { background: nextChip() } }), h('h2.shelf-title', {}, pixel(title)), h('span.shelf-rule'), toggle),
    body, more);
  parent.append(el);
  for (let i = 0; i < 8; i++) body.append(h('div.card.skeleton', {}, h('div.art'), h('div.card-title')));

  let shown = 0;
  let expanded = false;
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
