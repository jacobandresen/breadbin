// windows - the little windows of the desktop looks (themes/desktop.js and
// themes/dialup.js): icons drawn on whole pixels in the colours of the time,
// and a window with its title bar and its buttons along the foot, which Enter
// presses and whose bar it is dragged by, as on the desktops it pays tribute to.

import { h, showDialog, closeButton } from '../ui.js';

const ICONS = {
  // 32x32, drawn on whole pixels in the colours of the time
  computer: '<rect x="4" y="3" width="24" height="18" fill="#c0c0c0" stroke="#000"/><rect x="7" y="6" width="18" height="12" fill="#008080" stroke="#808080"/><path d="M9 9h6M9 12h10" stroke="#fff"/><rect x="11" y="21" width="10" height="3" fill="#808080"/><rect x="3" y="24" width="26" height="5" fill="#c0c0c0" stroke="#000"/><path d="M6 27h8" stroke="#000"/><rect x="23" y="26" width="3" height="1" fill="#008000"/>',
  folder: '<path d="M2 8h10l2 3h16v17H2z" fill="#ffff80" stroke="#000"/><path d="M2 12h28" stroke="#808000"/>',
  note: '<path d="M2 8h10l2 3h16v17H2z" fill="#ffff80" stroke="#000"/><path d="M14 25v-9l8-2v9" stroke="#000" stroke-width="2" fill="none"/><circle cx="12.5" cy="25" r="2" fill="#000"/><circle cx="20.5" cy="23" r="2" fill="#000"/>',
  demo: '<path d="M2 8h10l2 3h16v17H2z" fill="#ffff80" stroke="#000"/><rect x="9" y="15" width="14" height="10" fill="#000"/><path d="M10 17h4M10 23h12" stroke="#f0f"/><path d="M12 20h8" stroke="#0ff"/>',
  hack: '<rect x="3" y="4" width="26" height="24" fill="#fff" stroke="#000"/><rect x="3" y="4" width="26" height="4" fill="#000080"/><path d="M6 12h10M8 15h14M8 18h8M6 21h12M8 24h10" stroke="#000"/>',
  tray: '<rect x="1" y="2" width="9" height="7" fill="#c0c0c0" stroke="#000"/><rect x="3" y="4" width="5" height="3" class="led a" fill="#008080"/><rect x="6" y="7" width="9" height="7" fill="#c0c0c0" stroke="#000"/><rect x="8" y="9" width="5" height="3" class="led b" fill="#008080"/>',
  phone: '<rect x="3" y="18" width="26" height="9" fill="#c0c0c0" stroke="#000"/><rect x="6" y="21" width="2" height="2" fill="#0f0"/><rect x="10" y="21" width="2" height="2" fill="#808080"/><path d="M8 12c0-6 16-6 16 0l-3 3h-3v-3h-4v3h-3z" fill="#000"/><path d="M16 16v2" stroke="#000"/>',
  bin: '<path d="M8 9h16l-2 19H10z" fill="#fff" stroke="#000"/><path d="M6 7h20v2H6z" fill="#c0c0c0" stroke="#000"/><path d="M12 12l1 13M16 12v13M20 12l-1 13" stroke="#808080"/>',
  run: '<rect x="4" y="6" width="24" height="20" fill="#c0c0c0" stroke="#000"/><rect x="4" y="6" width="24" height="4" fill="#000080"/><path d="M9 16h10l-3-3M19 16l-3 3" stroke="#000" fill="none"/>',
  find: '<circle cx="13" cy="13" r="7" fill="#fff" stroke="#000" stroke-width="2"/><path d="M18 18l8 8" stroke="#000" stroke-width="3"/>',
  settings: '<rect x="4" y="5" width="24" height="22" fill="#c0c0c0" stroke="#000"/><path d="M9 11h14M9 16h14M9 21h14" stroke="#808080"/><rect x="12" y="9" width="3" height="4" fill="#000"/><rect x="18" y="14" width="3" height="4" fill="#000"/><rect x="10" y="19" width="3" height="4" fill="#000"/>',
  off: '<rect x="4" y="4" width="24" height="18" fill="#c0c0c0" stroke="#000"/><rect x="7" y="7" width="18" height="12" fill="#000"/><circle cx="16" cy="13" r="4" fill="none" stroke="#ff8000" stroke-width="2"/><path d="M16 8v5" stroke="#ff8000" stroke-width="2"/><rect x="10" y="24" width="12" height="4" fill="#808080"/>',
  toast: '<path d="M6 10a10 6 0 0 1 20 0v17H6z" fill="#fff" stroke="#000"/><path d="M13 14l6 4-6 4z" fill="#000"/>',
  puzzle: '<rect x="3" y="3" width="26" height="26" fill="#fff" stroke="#000"/><path d="M3 11.5h26M3 20.5h26M11.5 3v26M20.5 3v26" stroke="#000"/><rect x="21" y="21" width="8" height="8" fill="#000"/>',
  help: '<path d="M4 6h11l1 1 1-1h11v21H17l-1 1-1-1H4z" fill="#fff" stroke="#000"/><path d="M16 7v20" stroke="#000"/><path d="M7 11h6M7 14h6M7 17h5M19 11h6M19 14h6M19 17h4" stroke="#000080"/>',
  disk: '<rect x="2" y="8" width="28" height="16" fill="#fff" stroke="#000"/><path d="M2 19.5h28" stroke="#000"/><rect x="5" y="21" width="3" height="1" fill="#000"/><rect x="22" y="21" width="5" height="1" fill="#000"/>',
  trash: '<path d="M8 9h16l-2 20H10z" fill="#fff" stroke="#000"/><path d="M6 6h20v3H6zM13 6V4h6v2" fill="#fff" stroke="#000"/><path d="M12.5 12v14M16 12v14M19.5 12v14" stroke="#000"/>',
  text: '<path d="M7 3h13l6 6v20H7z" fill="#fff" stroke="#000"/><path d="M20 3v6h6" fill="#c0c0c0" stroke="#000"/><path d="M10 13h12M10 16h12M10 19h12M10 22h8" stroke="#000080"/>',
  speaker: '<path d="M4 12h6l8-7v22l-8-7H4z" fill="#ffff80" stroke="#000"/><path d="M22 11c2 2 2 8 0 10M25 8c4 4 4 12 0 16" stroke="#000" stroke-width="2" fill="none"/>',
  error: '<circle cx="16" cy="16" r="13" fill="#f00" stroke="#800000"/><path d="M11 11l10 10M21 11L11 21" stroke="#fff" stroke-width="3"/>',
};
/** Icon `name`, `size` pixels square, in a span. */
export const icon = (name, size = 32) => {
  const s = document.createElement('span');
  s.className = 'win-icon';
  s.innerHTML = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="' + size + '" height="' + size + '" shape-rendering="crispEdges" aria-hidden="true">' + ICONS[name] + '</svg>';
  return s;
};

/** A window of the time: its title on the bar, `body` in it, buttons along the foot. */
export function win(title, body, buttons, cls = '') {
  const dlg = h('dialog.sheet.win' + cls);
  const foot = h('div.win-buttons', {}, buttons.map(([label, fn, primary]) => h('button.key.pill' + (primary ? '.primary' : ''), { type: 'button', onclick: () => fn(dlg) }, label)));
  dlg.append(closeButton(dlg), h('div.sheet-body', {}, h('h2', {}, title), body, foot));
  // Enter presses the ringed button wherever the focus is in the window (but on another button, or in text to edit)
  dlg.addEventListener('keydown', (e) => {
    if (e.key !== 'Enter' || e.target.closest('button, textarea')) return;
    e.preventDefault();
    dlg.querySelector('.win-buttons .primary')?.click();
  });
  drag(dlg, dlg.querySelector('h2'));
  showDialog(dlg);
  (dlg.querySelector('.win-buttons .primary') || dlg.querySelector('.win-buttons button'))?.focus(); // (the button Enter presses has the focus, as it did)
  return dlg;
}

/** `dlg` moves with the pointer that holds `bar`, as a window did by its title bar (kept on the screen). */
function drag(dlg, bar) {
  bar.style.touchAction = 'none';
  bar.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    const [x0, y0] = (dlg.style.translate || '0px 0px').split(' ').map(parseFloat);
    const box = dlg.getBoundingClientRect();
    const start = [e.clientX, e.clientY];
    bar.setPointerCapture(e.pointerId);
    const move = (m) => {
      const dx = Math.min(Math.max(m.clientX - start[0], -box.left), innerWidth - box.right);
      const dy = Math.min(Math.max(m.clientY - start[1], -box.top), innerHeight - box.bottom);
      dlg.style.translate = `${x0 + dx}px ${y0 + (dy || 0)}px`;
    };
    const up = () => { bar.removeEventListener('pointermove', move); bar.removeEventListener('pointerup', up); };
    bar.addEventListener('pointermove', move);
    bar.addEventListener('pointerup', up);
  });
}

