// music-editor - the MUSIC mode: a tracker for the SID. Instruments on the
// left, the pattern being written in the middle, the songs on the right.
// Every sound is made by the game's own player (bbsound.s) on breadbin's SID
// engine, so it sounds here as it will in the game. Saves to
// assets/music.json (music.js).
//
// In the pattern, the computer keyboard is a piano, as in most trackers:
//   Z S X D C V G B H N J M ,   one octave (C to C)
//   Q 2 W 3 E R 5 T 6 Y 7 U I   the octave above
//   1 release · Delete clear · Space play / stop the pattern · Enter the song
//   in the instrument column 0-F; in the effect column U D V C, then hex digits

import { h, toast } from '../ui.js';
import { History } from './pixels.js';
import { parseStep, formatStep, noteName, EMPTY, MAX_STEPS, WAVES, defaultInstrument, songFrames } from './assets/music.js';
import { previewMusic } from './psid.js';
import { nextName } from './sprite-editor.js';

const PIANO = { z: 0, s: 1, x: 2, d: 3, c: 4, v: 5, g: 6, b: 7, h: 8, n: 9, j: 10, m: 11, ',': 12, q: 12, 2: 13, w: 14, 3: 15, e: 16, r: 17, 5: 18, t: 19, 6: 20, y: 21, 7: 22, u: 23, i: 24 };
const FX_KEYS = { u: 1, d: 2, v: 3, c: 4 };
const FX_NAMES = { 1: 'slide up', 2: 'slide down', 3: 'vibrato', 4: 'cut' };
const COLS = ['note', 'ins', 'fx'];

export class MusicEditor {
  constructor(studio) {
    this.studio = studio;
    this.music = null;
    this.ins = 0;
    this.pat = 0;
    this.song = 0;
    this.row = 0;
    this.col = 0;
    this.octave = 4;
    this.step = 1;
    this.playing = null; // { kind, start, speed, len, frames }
    this.history = new History(80);

    const u = (this.ui = {});
    u.playSong = h('button.key.pill.primary', { onclick: () => this.playSong(), title: 'Play the song (Enter)' }, '▶ Song');
    u.playPat = h('button.key.pill', { onclick: () => this.playPattern(), title: 'Play this pattern, looping (Space)' }, '▶ Pattern');
    u.stop = h('button.key.pill', { onclick: () => this.stop(), title: 'Stop' }, '■ Stop');
    u.octave = h('span.studio-octave');
    u.stepIn = h('input.studio-num', { type: 'number', min: 0, max: 16, value: 1, title: 'Rows to move down after entering a note', onchange: (e) => (this.step = Number(e.target.value) || 0) });
    u.pos = h('span.studio-playpos');
    const transport = h('div.studio-transport', {}, u.playSong, u.playPat, u.stop,
      h('span.studio-sep'),
      h('span.studio-small', {}, 'octave'), h('button.studio-tool', { onclick: () => this.setOctave(this.octave - 1), title: 'Octave down' }, '−'), u.octave,
      h('button.studio-tool', { onclick: () => this.setOctave(this.octave + 1), title: 'Octave up' }, '+'),
      h('span.studio-small', {}, ' step'), u.stepIn, h('span.studio-sep'), u.pos,
      h('button.studio-tool.studio-sidout', { onclick: () => this.studio.downloadSid(), title: 'The music as a .sid file (each song a sub-tune), for any SID player' }, '⇩ .sid'));

    u.insList = h('div.studio-musiclist');
    u.insEdit = h('div.studio-insedit');
    u.patList = h('div.studio-musiclist');
    u.patHead = h('div.studio-pathead');
    u.grid = h('div.studio-tracker', { tabIndex: 0 });
    u.songList = h('div.studio-musiclist');
    u.songRows = h('div.studio-songrows');
    u.help = h('p.studio-small.studio-trackerhelp', {}, 'Keyboard piano: Z S X D C V G B H N J M (and Q 2 W 3 E… an octave up) · 1 release · Del clear · Space play · in the I column 0-F · in FX: U D V C then hex');

    this.el = h('div.studio-music.studio-editor', { tabIndex: -1 },
      transport,
      h('div.studio-musiccols', {},
        h('section.studio-musiccol', {}, h('div.studio-files-head', {}, 'Instruments', h('button.studio-tool', { onclick: () => this.addIns() }, '+ new')), u.insList, u.insEdit),
        h('section.studio-musiccol.studio-patcol', {}, h('div.studio-files-head', {}, 'Patterns', h('button.studio-tool', { onclick: () => this.addPattern() }, '+ new'), h('button.studio-tool', { onclick: () => this.copyPattern() }, 'copy')),
          u.patList, u.patHead, u.grid, u.help),
        h('section.studio-musiccol', {}, h('div.studio-files-head', {}, 'Songs', h('button.studio-tool', { onclick: () => this.addSong() }, '+ new')), u.songList, u.songRows)));

    u.grid.addEventListener('keydown', (e) => this.gridKey(e));
    this.el.addEventListener('keydown', (e) => this.key(e));
    this.studio.sid.addEventListener('state', () => this.renderTransport());
    this.studio.sid.addEventListener('ended', () => this.stop());
  }

  load(music) {
    if (!music) return;
    this.music = structuredClone(music);
    this.music.instruments ??= [defaultInstrument()];
    this.music.patterns ??= [{ name: 'pattern 0', speed: 6, steps: Array(32).fill(EMPTY) }];
    this.music.songs ??= [];
    this.ins = Math.min(this.ins, this.music.instruments.length - 1);
    this.pat = Math.min(this.pat, this.music.patterns.length - 1);
    this.song = Math.min(this.song, Math.max(0, this.music.songs.length - 1));
    this.render();
  }

  show() {
    this.ui.grid.focus({ preventScroll: true });
  }

  save() {
    this.studio.commit('music', this.music);
  }

  change(fn) {
    this.history.push(this.music);
    fn();
    this.save();
    this.render();
  }

  render() {
    this.renderInstruments();
    this.renderPatterns();
    this.renderGrid();
    this.renderSongs();
    this.renderTransport();
    this.ui.octave.textContent = String(this.octave);
  }

  // ── instruments ───────────────────────────────────────────────────────────

  renderInstruments() {
    this.ui.insList.replaceChildren(...this.music.instruments.map((ins, i) => h('button.studio-musicitem', {
      'aria-selected': String(i === this.ins), onclick: () => ((this.ins = i), this.renderInstruments()),
    }, h('span.studio-hex', {}, i.toString(16).toUpperCase()), h('span', {}, ins.name), h('span.studio-wave', {}, ins.drum ? 'drum' : ins.wave))));
    const ins = this.music.instruments[this.ins];
    if (!ins) return;
    const set = (k, v) => this.change(() => (ins[k] = v));
    const slider = (k, label, max = 15, min = 0) => h('label.studio-slider', { title: label },
      h('span', {}, label), h('input', { type: 'range', min, max, value: ins[k] ?? 0, oninput: (e) => (e.target.nextSibling.textContent = e.target.value), onchange: (e) => set(k, Number(e.target.value)) }),
      h('output', {}, String(ins[k] ?? 0)));
    this.ui.insEdit.replaceChildren(
      h('div.studio-insrow', {},
        h('input.studio-input', { value: ins.name, spellcheck: false, onchange: (e) => set('name', e.target.value.trim() || `ins ${this.ins}`) }),
        h('button.key.pill', { onclick: () => this.audition(48 + 12 * (this.octave - 4)), title: 'Hear it' }, '▶'),
        h('button.studio-tool.danger', { onclick: () => this.removeIns(), title: 'Delete this instrument' }, '×')),
      h('div.studio-seg.studio-waves', {}, ...Object.keys(WAVES).filter((w) => !w.includes('+')).map((w) =>
        h('button', { 'aria-pressed': String(ins.wave === w), onclick: () => set('wave', w), title: w }, waveIcon(w), h('small', {}, w)))),
      h('div.studio-adsr', {}, slider('a', 'attack'), slider('d', 'decay'), slider('s', 'sustain'), slider('r', 'release')),
      h('div.studio-adsr', {}, slider('pw', 'pulse width'), slider('pwm', 'pulse sweep'), slider('vib', 'vibrato'), slider('slide', 'pitch slide', 60, -60)),
      h('div.studio-insrow', {},
        h('label.studio-check', { title: 'An arpeggio: the note, then these two notes above it (semitones), a frame each - a chord on one voice' }, 'chord +',
          h('input.studio-num', { type: 'number', min: 0, max: 15, value: ins.arp?.[0] ?? 0, onchange: (e) => set('arp', [Number(e.target.value) || 0, ins.arp?.[1] ?? 0]) }), ' +',
          h('input.studio-num', { type: 'number', min: 0, max: 15, value: ins.arp?.[1] ?? 0, onchange: (e) => set('arp', [ins.arp?.[0] ?? 0, Number(e.target.value) || 0]) })),
        h('label.studio-check', { title: 'A click of noise as the note starts: kicks and snares' }, h('input', { type: 'checkbox', checked: !!ins.drum, onchange: (e) => set('drum', e.target.checked) }), ' drum')),
      envelope(ins));
  }

  addIns() {
    if (this.music.instruments.length >= 16) return toast('16 instruments is the most a step can name (0-F).');
    this.change(() => {
      this.music.instruments.push({ ...structuredClone(this.music.instruments[this.ins] || defaultInstrument()), name: nextName('ins 1', this.music.instruments.map((i) => i.name)) });
      this.ins = this.music.instruments.length - 1;
    });
  }

  removeIns() {
    if (this.music.instruments.length === 1) return;
    if (!confirm(`Delete the instrument ${this.music.instruments[this.ins].name}? Notes that play it play instrument 0 instead.`)) return;
    this.change(() => {
      const gone = this.ins;
      this.music.instruments.splice(gone, 1);
      this.music.patterns = remapInstruments(this.music.patterns, gone);
      this.ins = Math.max(0, gone - 1);
    });
  }

  // ── patterns ──────────────────────────────────────────────────────────────

  get pattern() {
    return this.music.patterns[this.pat];
  }

  renderPatterns() {
    this.ui.patList.replaceChildren(...this.music.patterns.map((p, i) => h('button.studio-musicitem', {
      'aria-selected': String(i === this.pat), onclick: () => ((this.pat = i), (this.row = Math.min(this.row, p.steps.length - 1)), this.renderPatterns(), this.renderGrid()),
      title: `SFX_${p.name.toUpperCase().replace(/[^A-Z0-9]+/g, '_')} = ${i}`,
    }, h('span.studio-hex', {}, String(i).padStart(2, '0')), h('span', {}, p.name), h('span.studio-wave', {}, `${p.steps.length}×${p.speed}`))));
    const p = this.pattern;
    this.ui.patHead.replaceChildren(
      h('input.studio-input', { value: p.name, spellcheck: false, title: 'Its name: SFX_<NAME> in the code (bb_sfx)', onchange: (e) => this.change(() => (p.name = e.target.value.trim() || `pattern ${this.pat}`)) }),
      h('label.studio-check', { title: 'Frames per step: 6 is 125 steps a minute' }, 'speed ', h('input.studio-num', { type: 'number', min: 1, max: 32, value: p.speed, onchange: (e) => this.change(() => (p.speed = Math.max(1, Math.min(32, Number(e.target.value) || 6)))) })),
      h('label.studio-check', {}, 'steps ', h('input.studio-num', { type: 'number', min: 1, max: MAX_STEPS, value: p.steps.length, onchange: (e) => this.resize(Number(e.target.value)) })),
      h('button.studio-tool', { onclick: () => this.transpose(1), title: 'Every note a semitone up' }, '♯'),
      h('button.studio-tool', { onclick: () => this.transpose(-1), title: 'Every note a semitone down' }, '♭'),
      h('button.studio-tool.danger', { onclick: () => this.removePattern(), title: 'Delete this pattern' }, '×'));
  }

  renderGrid() {
    const p = this.pattern;
    const rows = p.steps.map((s, i) => {
      const st = parseStep(s);
      const cell = (col, text, cls = '') => h(`span.studio-cell.${col}${cls}`, { 'data-row': i, 'data-col': col, className: `studio-cell ${col}${cls}${i === this.row && COLS[this.col] === col ? ' cursor' : ''}` }, text);
      const note = st.release ? '===' : st.note == null ? '···' : noteName(st.note);
      const ins = st.ins == null ? '·' : st.ins.toString(16).toUpperCase();
      const fx = st.fx ? formatStep(st).split(' ')[2] : '···';
      return h('div.studio-step', { className: `studio-step${i % 4 === 0 ? ' beat' : ''}${i === this.row ? ' on' : ''}` },
        h('span.studio-stepnum', {}, String(i).padStart(2, '0')),
        cell('note', note, st.release ? ' rel' : st.note == null ? ' empty' : ''),
        cell('ins', ins, st.ins == null ? ' empty' : ''),
        cell('fx', fx, st.fx ? '' : ' empty'));
    });
    this.ui.grid.replaceChildren(...rows);
    this.ui.grid.onclick = (e) => {
      const c = e.target.closest('.studio-cell');
      if (!c) return;
      this.row = Number(c.dataset.row);
      this.col = COLS.indexOf(c.dataset.col);
      this.renderGrid();
      this.ui.grid.focus();
    };
    this.ui.grid.querySelector('.studio-step.on')?.scrollIntoView({ block: 'nearest' });
  }

  setStep(fn) {
    const p = this.pattern;
    this.history.push(this.music);
    const st = parseStep(p.steps[this.row]);
    fn(st);
    p.steps[this.row] = formatStep(st);
    this.save();
  }

  move(dr, dc = 0) {
    const n = this.pattern.steps.length;
    this.row = (this.row + dr + n) % n;
    this.col = Math.max(0, Math.min(2, this.col + dc));
    this.renderGrid();
  }

  gridKey(e) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const k = e.key.toLowerCase();
    const col = COLS[this.col];
    let handled = true;
    if (k === 'arrowup') this.move(-1);
    else if (k === 'arrowdown') this.move(1);
    else if (k === 'arrowleft') this.move(0, -1);
    else if (k === 'arrowright' || k === 'tab') this.move(0, 1);
    else if (k === 'pageup') this.move(-4);
    else if (k === 'pagedown') this.move(4);
    else if (k === 'delete' || k === 'backspace') {
      this.setStep((st) => {
        if (col === 'note') Object.assign(st, { note: null, release: false, ins: null, fx: 0, param: 0 });
        else if (col === 'ins') st.ins = null;
        else Object.assign(st, { fx: 0, param: 0 });
      });
      this.move(this.step);
    } else if (col === 'note' && k === '1') {
      this.setStep((st) => Object.assign(st, { note: null, release: true, ins: null }));
      this.move(this.step);
    } else if (col === 'note' && k in PIANO) {
      const n = Math.max(0, Math.min(95, this.octave * 12 + PIANO[k]));
      this.setStep((st) => Object.assign(st, { note: n, release: false, ins: this.ins }));
      this.audition(n);
      this.move(this.step);
    } else if (col === 'ins' && /^[0-9a-f]$/.test(k)) {
      const v = parseInt(k, 16);
      if (v < this.music.instruments.length) this.setStep((st) => (st.ins = v));
      this.move(this.step);
    } else if (col === 'fx' && k in FX_KEYS) {
      this.setStep((st) => ((st.fx = FX_KEYS[k]), (st.param = st.param || 4)));
      this.renderGrid();
      toast(`${FX_NAMES[FX_KEYS[k]]}: now type its amount in hex`, 2000);
    } else if (col === 'fx' && k === '.') {
      this.setStep((st) => Object.assign(st, { fx: 0, param: 0 }));
      this.renderGrid();
    } else if (col === 'fx' && /^[0-9a-f]$/.test(k)) {
      this.setStep((st) => {
        if (!st.fx) st.fx = 3;
        st.param = ((st.param << 4) | parseInt(k, 16)) & 0xff;
      });
      this.renderGrid();
    } else if (k === ' ') this.playing ? this.stop() : this.playPattern();
    else if (k === 'enter') this.playSong();
    else handled = false;
    if (handled) e.preventDefault();
  }

  addPattern() {
    this.change(() => {
      this.music.patterns.push({ name: nextName('pattern 1', this.music.patterns.map((p) => p.name)), speed: this.pattern?.speed || 6, steps: Array(32).fill(EMPTY) });
      this.pat = this.music.patterns.length - 1;
      this.row = 0;
    });
  }

  copyPattern() {
    this.change(() => {
      const p = structuredClone(this.pattern);
      p.name = nextName(p.name, this.music.patterns.map((x) => x.name));
      this.music.patterns.push(p);
      this.pat = this.music.patterns.length - 1;
    });
  }

  removePattern() {
    if (this.music.patterns.length === 1) return;
    const used = this.music.songs.some((s) => s.rows.some((r) => r.includes(this.pat)));
    if (!confirm(`Delete the pattern ${this.pattern.name}?${used ? ' Songs that play it will rest there instead.' : ''}`)) return;
    this.change(() => {
      const gone = this.pat;
      this.music.patterns.splice(gone, 1);
      // songs: the gone pattern rests, the later ones move up a number
      for (const s of this.music.songs) s.rows = s.rows.map((r) => r.map((v) => (v === gone ? -1 : v > gone ? v - 1 : v)));
      this.pat = Math.max(0, gone - 1);
    });
  }

  resize(n) {
    const len = Math.max(1, Math.min(MAX_STEPS, n || 32));
    this.change(() => {
      const s = this.pattern.steps;
      while (s.length < len) s.push(EMPTY);
      s.length = len;
      this.row = Math.min(this.row, len - 1);
    });
  }

  transpose(d) {
    this.change(() => {
      this.pattern.steps = this.pattern.steps.map((s) => {
        const st = parseStep(s);
        if (st.note != null) st.note = Math.max(0, Math.min(95, st.note + d));
        return formatStep(st);
      });
    });
  }

  // ── songs ─────────────────────────────────────────────────────────────────

  renderSongs() {
    const songs = this.music.songs;
    this.ui.songList.replaceChildren(...songs.map((s, i) => h('button.studio-musicitem', {
      'aria-selected': String(i === this.song), onclick: () => ((this.song = i), this.renderSongs()),
      title: `SONG_${s.name.toUpperCase().replace(/[^A-Z0-9]+/g, '_')} = ${i}`,
    }, h('span.studio-hex', {}, String(i)), h('span', {}, s.name), h('span.studio-wave', {}, `${(songFrames(this.music, s) / 50).toFixed(0)} s`))));
    const s = songs[this.song];
    if (!s) return this.ui.songRows.replaceChildren(h('p.studio-small', {}, 'No songs yet: + new makes one.'));
    const options = (v) => [h('option', { value: -1, selected: v < 0 }, '—'), ...this.music.patterns.map((p, i) => h('option', { value: i, selected: i === v }, `${String(i).padStart(2, '0')} ${p.name}`))];
    this.ui.songRows.replaceChildren(
      h('div.studio-insrow', {},
        h('input.studio-input', { value: s.name, spellcheck: false, title: 'Its name: SONG_<NAME> in the code (bb_music)', onchange: (e) => this.change(() => (s.name = e.target.value.trim() || `song ${this.song}`)) }),
        h('button.studio-tool.danger', { onclick: () => this.removeSong(), title: 'Delete this song' }, '×')),
      h('div.studio-songhead', {}, h('span', {}, ''), h('span', {}, 'voice 1'), h('span', {}, 'voice 2'), h('span', {}, 'voice 3'), h('span', {}, '')),
      ...s.rows.map((r, i) => h('div.studio-songrow', { className: `studio-songrow${s.loop === i ? ' loop' : ''}` },
        h('button.studio-rownum', { title: s.loop === i ? 'The song loops back to here' : 'Loop back to this row at the end', onclick: () => this.change(() => (s.loop = s.loop === i ? -1 : i)) }, s.loop === i ? '↻' : String(i)),
        ...[0, 1, 2].map((v) => h('select.studio-select', { onchange: (e) => this.change(() => (r[v] = Number(e.target.value))) }, ...options(r[v] ?? -1))),
        h('button.studio-tool', { onclick: () => this.change(() => s.rows.splice(i, 1)), title: 'Remove this row' }, '×'))),
      h('div.studio-insrow', {},
        h('button.studio-tool', { onclick: () => this.change(() => s.rows.push([...(s.rows.at(-1) || [this.pat, -1, -1])])) }, '+ row'),
        h('span.studio-small', {}, s.loop >= 0 ? `loops to row ${s.loop}` : 'plays once (click a row number to loop there)')));
  }

  addSong() {
    this.change(() => {
      this.music.songs.push({ name: nextName('song 1', this.music.songs.map((s) => s.name)), loop: 0, rows: [[this.pat, -1, -1]] });
      this.song = this.music.songs.length - 1;
    });
  }

  removeSong() {
    if (!confirm(`Delete the song ${this.music.songs[this.song].name}?`)) return;
    this.change(() => {
      this.music.songs.splice(this.song, 1);
      this.song = Math.max(0, this.song - 1);
    });
  }

  // ── sound ─────────────────────────────────────────────────────────────────

  async audition(note) {
    this.playing = null;
    try {
      await this.studio.playMusic(previewMusic(this.music, { note, ins: this.ins }));
    } catch (e) {
      toast(`No sound: ${e.message}`);
    }
    this.renderTransport();
  }

  async playPattern() {
    const p = this.pattern;
    await this.play(previewMusic(this.music, { pattern: this.pat }), { kind: 'pattern', speed: p.speed, len: p.steps.length });
  }

  async playSong() {
    if (!this.music.songs[this.song]) return toast('Make a song first (+ new, under Songs).');
    await this.play(previewMusic(this.music, { song: this.song }), { kind: 'song' });
  }

  /**
   * Play `music`, following it as `info` says. Playing counts from the
   * press, not from when the sound starts (it takes a moment to assemble):
   * a second press, or Stop, in that moment stops it instead.
   */
  async play(music, info) {
    const mine = { ...info, pending: true };
    this.playing = mine;
    this.renderTransport();
    try {
      await this.studio.playMusic(music);
    } catch (e) {
      toast(`No sound: ${e.message}`);
      if (this.playing === mine) this.playing = null;
      return this.renderTransport();
    }
    if (this.playing !== mine) {
      // stopped meanwhile: the sound that just started goes too (something
      // else started instead: that one plays)
      if (!this.playing) this.studio.sid.stop();
      return;
    }
    mine.pending = false;
    mine.start = this.studio.sid.frame;
    this.tickPos();
  }

  stop() {
    if (this.playing?.pending) this.playing = null; // it stops as it starts
    this.studio.sid.stop();
    this.playing = null;
    cancelAnimationFrame(this.raf);
    this.renderTransport();
    for (const el of this.ui.grid.querySelectorAll('.playing')) el.classList.remove('playing');
  }

  /** Follow the playing step in the grid. */
  tickPos() {
    cancelAnimationFrame(this.raf);
    const tick = () => {
      if (!this.playing || this.playing.pending) return;
      const f = this.studio.sid.frame - this.playing.start;
      if (this.playing.kind === 'pattern') {
        const stepNo = Math.floor(Math.max(0, f) / this.playing.speed) % this.playing.len;
        for (const [i, el] of [...this.ui.grid.children].entries()) el.classList.toggle('playing', i === stepNo);
        this.ui.pos.textContent = `step ${String(stepNo).padStart(2, '0')}`;
      } else {
        this.ui.pos.textContent = `${(Math.max(0, f) / 50).toFixed(1)} s`;
      }
      this.raf = requestAnimationFrame(tick);
    };
    tick();
    this.renderTransport();
  }

  renderTransport() {
    const on = !!this.playing && (this.playing.pending || this.studio.sid.playing);
    this.ui.stop.disabled = !this.studio.sid.playing && !this.playing;
    this.ui.playPat.setAttribute('aria-pressed', String(on && this.playing.kind === 'pattern'));
    this.ui.playSong.classList.toggle('on', on && this.playing.kind === 'song');
    if (!on) this.ui.pos.textContent = '';
  }

  setOctave(o) {
    this.octave = Math.max(0, Math.min(7, o));
    this.ui.octave.textContent = String(this.octave);
  }

  key(e) {
    if (e.target.matches('input, select, textarea')) return;
    const k = e.key.toLowerCase();
    if ((e.ctrlKey || e.metaKey) && k === 'z') {
      e.preventDefault();
      const s = e.shiftKey ? this.history.redo(this.music) : this.history.undo(this.music);
      if (s) (this.music = s), this.save(), this.render();
    } else if ((e.ctrlKey || e.metaKey) && k === 'y') {
      e.preventDefault();
      const s = this.history.redo(this.music);
      if (s) (this.music = s), this.save(), this.render();
    } else if (e.target === this.ui.grid) {
      // the grid handles its own keys
    } else if (k === '*' || k === '+') this.setOctave(this.octave + 1);
    else if (k === '/' || k === '-') this.setOctave(this.octave - 1);
  }
}

/**
 * The patterns after instrument `gone` is deleted: its notes play instrument
 * 0, and the instruments after it move down a number, as their list did.
 */
export function remapInstruments(patterns, gone) {
  return patterns.map((p) => ({
    ...p,
    steps: p.steps.map((s) => {
      const st = parseStep(s);
      if (st.ins == null) return s;
      st.ins = st.ins === gone ? 0 : st.ins > gone ? st.ins - 1 : st.ins;
      return formatStep(st);
    }),
  }));
}

function waveIcon(w) {
  const paths = { tri: 'M1 9 L6 2 L11 9 L16 2', saw: 'M1 9 L8 2 L8 9 L15 2 L15 9', pulse: 'M1 9 L1 2 L6 2 L6 9 L11 9 L11 2 L16 2', noise: 'M1 6 L3 2 L4 8 L6 3 L7 9 L9 1 L10 7 L12 3 L13 8 L15 4 L16 6' };
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 17 11');
  svg.setAttribute('class', 'studio-waveicon');
  const p = document.createElementNS('http://www.w3.org/2000/svg', 'path');
  p.setAttribute('d', paths[w]);
  svg.append(p);
  return svg;
}

/** The instrument's envelope, drawn: attack, decay to sustain, release. */
function envelope(ins) {
  const W = 220;
  const H = 54;
  // SID times grow fast with the setting; draw them on a gentle curve
  const t = (v) => 6 + v * v * 0.55;
  const a = t(ins.a);
  const d = t(ins.d);
  const r = t(ins.r);
  const hold = 40;
  const total = a + d + hold + r;
  const k = (W - 4) / total;
  const sus = H - 4 - ((ins.s ?? 0) / 15) * (H - 8);
  const pts = [[2, H - 2], [2 + a * k, 4], [2 + (a + d) * k, sus], [2 + (a + d + hold) * k, sus], [2 + total * k, H - 2]];
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', `0 0 ${W} ${H}`);
  svg.setAttribute('class', 'studio-envelope');
  const area = document.createElementNS('http://www.w3.org/2000/svg', 'path');
  area.setAttribute('d', `M${pts.map((p) => p.join(' ')).join(' L')} Z`);
  area.setAttribute('class', 'fill');
  const line = document.createElementNS('http://www.w3.org/2000/svg', 'path');
  line.setAttribute('d', `M${pts.map((p) => p.join(' ')).join(' L')}`);
  line.setAttribute('class', 'line');
  svg.append(area, line);
  return svg;
}
