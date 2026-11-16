// lessons - short guided changes to the Empty game, step by step, in a card
// under the toolbar. Each step says what to do, can do it for you ("Show
// me": the change the editors would make), and knows when it is done (a
// look at the files, or at the running game's variables), then moves on.
//
// The coins lesson is the guide's tutorial (tools/studio/README.md, "Your
// first change"), as tools/studio/test/build/tutorial.test.mjs does it.

import { setCell, roomTiles } from './assets/map.js';
import { parseStep, formatStep, parseNote } from './assets/music.js';
import { pretty } from './project.js';
import { h } from '../ui.js';

const json = (files, p) => {
  try {
    return JSON.parse(files[p]);
  } catch {
    return null;
  }
};
const MAIN = 'src/main.c';
const COIN_CODE = `    /* a coin under the hero's middle: take it */
    if (bb_flags_at(x + 12, y + 12) & FLAG_PICKUP) {
      bb_set_tile((x + 12) >> 3, (y + 12) >> 3, 32);   /* a space where it was */
      bb_sfx(SFX_COIN);
      bb_num(34, 1, ++coins, 3, BB_YELLOW);
    }

`;

export const LESSONS = [
  {
    id: 'jump',
    title: 'Your first knob: the jump',
    about: 'Make the hero\'s jump a setting, then tune it while the game runs.',
    steps: [
      {
        text: 'In src/main.c the jump is the -6 in "fall = -6;". Make it a setting: a line "signed char jump = -6;" under "signed char fall;", and "fall = jump;" where the -6 was.',
        file: MAIN,
        show: (f) => ({ [MAIN]: f[MAIN].replace('signed char fall;', 'signed char fall;\nsigned char jump = -6;    /* how hard the hero jumps: more minus, higher */').replace('fall = -6;', 'fall = jump;') }),
        done: ({ files }) => /^signed char jump = -?\d+;/m.test(files[MAIN] || '') && /fall = jump;/.test(files[MAIN] || ''),
      },
      {
        text: 'Press ▶ Run (Ctrl+Enter). Jump with the joystick (cursor up). The number beside "jump" in the code is the game\'s, live.',
        done: ({ vars, running }) => running && vars.some((v) => v.name === 'jump'),
      },
      {
        text: 'Under the C64, the Tuning panel has a slider for jump. Drag it to -9 or lower and jump again: higher! Let go, and the code keeps the number.',
        done: ({ files }) => Number(/^signed char jump = (-?\d+);/m.exec(files[MAIN] || '')?.[1] ?? 0) <= -9,
      },
    ],
  },
  {
    id: 'coins',
    title: 'Coins to collect',
    about: 'A coin tile, coins in the room, a sound and a counter: a little of every editor.',
    steps: [
      {
        text: 'TILES (Alt+3): click the empty slot 66, paint a coin in yellow, name it "coin" and tick its "pickup" flag.',
        mode: 'tiles',
        show: (f) => {
          const t = json(f, 'assets/tiles.json');
          const pickup = t.flags.indexOf('pickup');
          t.tiles[66] = { name: 'coin', multi: false, color: 7, flags: 1 << pickup, rows: ['..1111..', '.111111.', '11.11.11', '11.11.11', '11.11.11', '11.11.11', '.111111.', '..1111..'] };
          return { 'assets/tiles.json': `${pretty(t)}\n` };
        },
        done: ({ files }) => {
          const t = json(files, 'assets/tiles.json');
          const pickup = t?.flags?.indexOf('pickup') ?? -1;
          return !!t?.tiles?.some((x) => x && /coin/i.test(x.name || '') && pickup >= 0 && x.flags & (1 << pickup));
        },
      },
      {
        text: 'MAP (Alt+4): pick the coin from the palette on the right and click a few into the room, above the ground.',
        mode: 'map',
        show: (f) => {
          const m = json(f, 'assets/map.json');
          const coin = json(f, 'assets/tiles.json').tiles.findIndex((x) => x && /coin/i.test(x.name || ''));
          for (const x of [4, 10, 20, 28]) setCell(m, m.rooms[0], x, 21, coin);
          return { 'assets/map.json': `${pretty(m)}\n` };
        },
        done: ({ files }) => {
          const coin = json(files, 'assets/tiles.json')?.tiles?.findIndex((x) => x && /coin/i.test(x.name || ''));
          const m = json(files, 'assets/map.json');
          return coin > 0 && !!m?.rooms?.some((r) => roomTiles(m, r).includes(coin));
        },
      },
      {
        text: 'MUSIC (Alt+5): under Patterns, + new, name it "coin", speed 2. Click the first note cell and press Q, E, T, then 1 (C-5, E-5, G-5, release): that is SFX_COIN.',
        mode: 'music',
        show: (f) => {
          const m = json(f, 'assets/music.json');
          const steps = Array(8).fill('--- . ...');
          ['C-5', 'E-5', 'G-5'].forEach((n, i) => (steps[i] = formatStep({ ...parseStep(steps[i]), note: parseNote(n), ins: 0 })));
          steps[3] = '=== . ...';
          m.patterns.push({ name: 'coin', speed: 2, steps });
          return { 'assets/music.json': `${pretty(m)}\n` };
        },
        done: ({ files }) => !!json(files, 'assets/music.json')?.patterns?.some((p) => /coin/i.test(p.name || '')),
      },
      {
        text: 'CODE: in src/main.c a counter, "unsigned char coins;" under "unsigned char left, walk;", and in the loop, before the fire button\'s line, the code that takes a coin (Show me writes it).',
        file: MAIN,
        show: (f) => ({ [MAIN]: f[MAIN].replace('unsigned char left, walk;', 'unsigned char left, walk;\nunsigned char coins;').replace('    if (bb_btnp(BB_FIRE)) bb_sfx(SFX_BLIP);', `${COIN_CODE}    if (bb_btnp(BB_FIRE)) bb_sfx(SFX_BLIP);`) }),
        done: ({ files }) => /unsigned char coins;/.test(files[MAIN] || '') && /FLAG_PICKUP/.test(files[MAIN] || '') && /\+\+coins/.test(files[MAIN] || ''),
      },
      {
        text: '▶ Run, and walk into a coin: blip, and the counter at the top goes up. Well done!',
        done: ({ value }) => Number(value('coins')) > 0,
      },
    ],
  },
];

/** The lesson card: the step, Show me, Next (when the step is done), and the lesson's progress. */
export class LessonCard {
  constructor({ onShow, onExit, onStep }) {
    this.onShow = onShow;
    this.onExit = onExit;
    this.onStep = onStep;
    this.lesson = null;
    this.step = 0;
    this.el = h('div.studio-lesson', { hidden: true });
  }

  start(lesson) {
    this.lesson = lesson;
    this.step = 0;
    this.el.hidden = false;
    this.render(false);
  }

  stop() {
    this.lesson = null;
    this.el.hidden = true;
  }

  /** Is the step done, by `ctx` ({ files, vars, running, value(name) })? Moves on when it is. */
  check(ctx) {
    const st = this.lesson?.steps[this.step];
    if (!st) return;
    let ok = false;
    try {
      ok = !!st.done(ctx);
    } catch {}
    if (ok !== this.done) this.render(ok);
  }

  next() {
    if (!this.lesson) return;
    if (this.step + 1 >= this.lesson.steps.length) return this.finish();
    this.step++;
    this.render(false);
    this.onStep?.(this.lesson.steps[this.step]);
  }

  finish() {
    const l = this.lesson;
    this.el.replaceChildren(h('div.studio-lesson-head', {}, h('b', {}, `🎓 ${l.title}`), h('span', {}, 'done')),
      h('p', {}, 'That\'s the lesson. The project is yours: change anything, Ctrl+Enter runs it.'),
      h('div.studio-lesson-acts', {}, h('button.key.pill.small.primary', { onclick: () => (this.stop(), this.onExit?.()) }, 'Close')));
  }

  render(done) {
    const l = this.lesson;
    const st = l.steps[this.step];
    this.done = done;
    const last = this.step + 1 === l.steps.length;
    this.el.classList.toggle('done', done);
    this.el.replaceChildren(
      h('div.studio-lesson-head', {}, h('b', {}, `🎓 ${l.title}`), h('span.studio-lesson-n', {}, `${this.step + 1} / ${l.steps.length}`),
        h('button.studio-lesson-x', { onclick: () => (this.stop(), this.onExit?.()), title: 'Leave the lesson (the project stays)', 'aria-label': 'Leave the lesson' }, '×')),
      h('p', {}, done ? `✓ ${st.text}` : st.text),
      h('div.studio-lesson-acts', {},
        st.show ? h('button.key.pill.small', { onclick: () => this.onShow?.(st), disabled: done, title: 'Make this change for me' }, 'Show me') : null,
        h('button.key.pill.small.primary.studio-lesson-next', { onclick: () => this.next(), disabled: !done }, last ? 'Finish' : 'Next →')));
  }
}
