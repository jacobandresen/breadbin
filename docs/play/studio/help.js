// help - the cheat sheet (F1 or ?): the runtime's API from bb.h, grouped as
// the header groups it, with what each call does, all on one page.

import { h } from '../ui.js';

const GROUPS = [
  ['The frame', /^bb_(init|frame|frames|colors)$/],
  ['Joystick', /^bb_(joy|btn|btnp|joy_now|joy_was)$|^BB_(UP|DOWN|LEFT|RIGHT|FIRE)$/],
  ['Sprites', /^bb_(spr|spr_hide|spr_color|overlap)$/],
  ['Tiles and rooms', /^bb_(room|tile|set_tile|fget|flags_at|tile_flags)$/],
  ['Text', /^bb_(text|num|cls)$/],
  ['Sound', /^bb_(music|sfx)$|^BB_STOP$/],
  ['Pixels: the bitmap', /^bb_(bitmap|textmode|pen|pen_mode|plot|pget|line|rect|fill|circle|disc|ink|btext)$|^BB_(DRAW|ERASE|FLIP|COPY)$/],
  ['Blits', /^bb_(blit|grab|tile_picture)$/],
  ['Memory', /^bb_(peek|poke|peekw|pokew)$/],
  ['Odds and ends', /^bb_(rnd|seed)$/],
  ['Colours', /^BB_(BLACK|WHITE|RED|CYAN|PURPLE|GREEN|BLUE|YELLOW|ORANGE|BROWN|PINK|DARKGREY|GREY|LIGHTGREEN|LIGHTBLUE|LIGHTGREY)$/],
];

const code = (doc) => doc.split('`').map((part, i) => (i % 2 ? h('code', {}, part) : part));

export function helpSheet(api) {
  const dlg = h('dialog.sheet.studio-help');
  const close = () => dlg.close();
  const sections = GROUPS.map(([title, re]) => {
    const items = api.filter((e) => re.test(e.name));
    if (title === 'Colours') {
      return h('section.studio-help-group', {}, h('h3', {}, title), h('div.studio-help-colours', {},
        ...items.map((e, i) => h('span.studio-help-colour', {}, h('i', { style: { background: `var(--c64-${i})` } }), e.name))));
    }
    // constants that share a description (BB_UP, BB_DOWN...) go on one line
    const runs = [];
    for (const e of items) {
      const last = runs.at(-1);
      if (last && e.kind === 'constant' && last[0].kind === 'constant' && last[0].doc === e.doc) last.push(e);
      else runs.push([e]);
    }
    return h('section.studio-help-group', {}, h('h3', {}, title), ...runs.map((run) => h('div.studio-help-item', {},
      h('div.studio-help-sigs', {}, ...run.map((e) => h('code.studio-help-sig', {}, e.signature.replace(/^(const |volatile )/, '')))),
      run[0].doc ? h('p', {}, ...code(run[0].doc)) : null)));
  });
  dlg.append(
    h('header.studio-picker-head', {}, h('h2', {}, 'bb.h · the cheat sheet'), h('button.key.icon.small.close', { onclick: close, 'aria-label': 'Close' }, '×')),
    h('p.studio-help-intro', {}, '#include "bb.h" and "assets.h", call ', h('code', {}, 'bb_init()'), ' once, then ', h('code', {}, 'bb_frame()'), ' at the top of a loop that never ends. ',
      'The names your assets get (SPR_, TILE_, FLAG_, ROOM_, SONG_, SFX_) are in gen/assets.h, and the editor completes them.'),
    h('div.studio-help-grid', {}, ...sections),
    h('p.studio-small', {}, 'Keys: Ctrl+Enter build and run · Ctrl+S save · Alt+1…5 the modes · F12 or Ctrl+click go to a definition · Ctrl+F find · Ctrl+Space complete · Ctrl+/ comment'),
  );
  dlg.addEventListener('click', (e) => e.target === dlg && close());
  return dlg;
}
