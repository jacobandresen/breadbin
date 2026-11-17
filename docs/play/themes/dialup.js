// dialup - Chicago's Dial-Up Networking (opened from themes/desktop.js): it
// takes a user name and a number, dials BreadNet with the modem's whole song
// (dial tone, the number in DTMF, the ring, the answer and the handshake,
// synthesised), shows the AT talk between modem and computer, puts two little
// screens in the tray - and never gets online: there is nothing to connect to
// in 1995. Each failure comes at its own point in the call.

import { h } from '../ui.js';
import { settings } from '../settings.js';
import { icon, win } from './windows.js';

/** How it fails, each at its own point in the call: what the modem answers there, and what the window says. */
const FAILS = [
  { how: 'busy', modem: 'BUSY', why: 'The line is busy. Everyone else is online, downloading a picture of a cat (40 KB, 6 minutes).' },
  { how: 'noanswer', modem: 'NO ANSWER', why: 'The computer you are dialing into is not answering. It is a breadbin: it has no modem.' },
  { how: 'nodialtone', modem: 'NO DIALTONE', why: 'There is no dial tone. Make sure the phone cord is plugged into the wall, and that it is 1995.' },
  { how: 'full', modem: 'NO CARRIER', why: 'Someone picked up the phone in the kitchen.' },
  { how: 'full', modem: 'NO CARRIER', why: 'Error 691: your user name and password were not recognised. (Neither was the year.)' },
];

let sound = null;

export function dialUp() {
  const field = (label, input) => h('label.win-field', {}, h('span', {}, label), input);
  const user = h('input', { type: 'text', value: 'breadbin', autocomplete: 'off', spellcheck: false });
  const pass = h('input', { type: 'password', value: 'toaster', autocomplete: 'off' });
  const phone = h('input', { type: 'text', value: '555-0164', autocomplete: 'off', inputmode: 'tel' });
  win('Connect To', h('div.dialup-form', {},
    h('div.win-row', {}, icon('phone'), h('b', {}, 'BreadNet')),
    field('User name:', user),
    field('Password:', pass),
    h('label.check', {}, h('input', { type: 'checkbox', checked: true }), 'Save password'),
    field('Phone number:', phone),
    field('Dialing from:', h('select', {}, h('option', {}, 'Default Location'), h('option', {}, 'Grandma’s house')))),
  [['Connect', (dlg) => { dlg.close(); connect(phone.value); }, true], ['Cancel', (d) => d.close()]], '.dialup');
}

function connect(number) {
  const status = h('p.dialup-status', {}, 'Dialing\u2026');
  const log = h('pre.dialup-log', { 'aria-label': 'Modem' });
  const fail = FAILS[Math.floor(Math.random() * FAILS.length)];
  const dlg = win('Connecting to BreadNet', h('div', {},
    h('div.win-row', {}, h('div.dialup-anim', { 'aria-hidden': 'true' }, icon('computer'), h('i'), icon('phone')), h('div', {}, h('p', {}, 'Status:'), status)),
    log),
  [['Cancel', (d) => d.close()]], '.dialup');
  const steps = modem(number, fail.how);
  // the tray's two little screens, blinking while it talks
  const tray = icon('tray', 16);
  tray.classList.add('tray-modem');
  tray.title = 'Connecting to BreadNet\u2026';
  document.querySelector('.theme-clock')?.before(tray);
  // what the modem says to the computer, as the terminal window showed it
  const say = (at, line, then) => setTimeout(() => { log.textContent += line + '\n'; log.scrollTop = log.scrollHeight; then?.(); }, at * 1000);
  const timers = [say(0, 'ATZ'), say(0.4, 'OK')];
  if (fail.how !== 'nodialtone') timers.push(say(1.2, `ATDT ${number}`));
  if (steps.ring) timers.push(say(steps.ring, 'RING', () => (status.textContent = 'Waiting for an answer\u2026')));
  if (steps.answer) timers.push(say(steps.answer, 'CONNECT 28800', () => (status.textContent = 'Verifying user name and password\u2026')));
  timers.push(
    say(steps.end - 0.6, fail.modem),
    setTimeout(() => { dlg.close(); failed(fail.why, number); }, steps.end * 1000),
  );
  dlg.addEventListener('close', () => { timers.forEach(clearTimeout); hush(); tray.remove(); });
}

function failed(why, number) {
  win('Dial-Up Networking', h('div.win-row', {}, icon('error'), h('div', {}, h('p', {}, 'Could not connect to BreadNet:'), h('p', {}, why))),
    [['Redial', (d) => { d.close(); connect(number); }, true], ['Close', (d) => d.close()]], '.dialup');
}

/** The modem quiet (its window closed, or the look gone). */
export function hush() {
  sound?.close().catch(() => {});
  sound = null;
}

/** Two instruments on `ctx`: tone(freqs, at, len, level, wave) and noise(at, len, level, band), `at` seconds from now. */
function synth(ctx) {
  const out = ctx.createGain();
  out.gain.value = 0.13 * settings.volume; // (as loud as Settings' volume says, and gentle: it is a screech)
  out.connect(ctx.destination);
  const t0 = ctx.currentTime + 0.05;
  const tone = (freqs, at, len, level = 0.5, type = 'sine') => {
    const g = ctx.createGain();
    g.gain.setValueAtTime(0, t0 + at);
    g.gain.linearRampToValueAtTime(level, t0 + at + 0.01);
    g.gain.setValueAtTime(level, t0 + at + len - 0.01);
    g.gain.linearRampToValueAtTime(0, t0 + at + len);
    g.connect(out);
    for (const f of freqs) {
      const o = ctx.createOscillator();
      o.type = type;
      o.frequency.value = f;
      o.connect(g);
      o.start(t0 + at);
      o.stop(t0 + at + len + 0.02);
    }
    return g;
  };
  const noise = (at, len, level, freq) => {
    const buf = ctx.createBuffer(1, ctx.sampleRate * len, ctx.sampleRate);
    const d = buf.getChannelData(0);
    for (let i = 0; i < d.length; i++) d[i] = Math.random() * 2 - 1;
    const src = ctx.createBufferSource();
    src.buffer = buf;
    const band = ctx.createBiquadFilter();
    band.type = 'bandpass';
    band.frequency.value = freq;
    band.Q.value = 0.8;
    const g = ctx.createGain();
    g.gain.value = level;
    src.connect(band).connect(g).connect(out);
    src.start(t0 + at);
  };
  return { tone, noise };
}

/** The modem's song, played now, as far as `how` the call gets: 'nodialtone' (silence), 'busy' (the busy signal after
 *  the number), 'noanswer' (it rings on), or 'full' (it rings, an answer, the handshake). When each part starts (s). */
function modem(number, how) {
  hush();
  let ctx;
  try { ctx = sound = new AudioContext(); } catch { return { ring: 2, answer: how === 'full' && 5, end: 10 }; }
  const { tone, noise } = synth(ctx);
  if (how === 'nodialtone') return { end: 3 }; // (the line is dead: nothing at all)
  // the dial tone, picked up
  let t = 0;
  tone([350, 440], t, 1.1, 0.4);
  t += 1.3;
  // the number, a tone of two notes a digit (DTMF)
  const ROW = [697, 770, 852, 941];
  const COL = [1209, 1336, 1477];
  const KEYS = '123456789*0#';
  for (const ch of String(number).replace(/[^0-9*#]/g, '').slice(0, 11)) {
    const k = KEYS.indexOf(ch);
    tone([ROW[Math.floor(k / 3)], COL[k % 3]], t, 0.09, 0.35);
    t += 0.16;
  }
  t += 0.3;
  if (how === 'busy') {
    for (let i = 0; i < 5; i++) tone([480, 620], t + i, 0.5, 0.3); // half a second on, half off
    return { end: t + 5 };
  }
  // it rings at the other end
  const ring = t;
  const rings = how === 'noanswer' ? 4 : 1;
  for (let i = 0; i < rings; i++) tone([440, 480], ring + i * 3, 1.6, 0.3);
  t = ring + (rings - 1) * 3 + 2.4;
  if (how === 'noanswer') return { ring, end: t };
  // an answer: the long, high tone, then the handshake - the bong, chirps up and down, then the hiss of the two modems learning the line
  const answer = t;
  tone([2100], t, 1.4, 0.35);
  t += 1.5;
  tone([1375, 2002], t, 0.5, 0.3);
  t += 0.6;
  for (let i = 0; i < 6; i++) tone([980 + (i % 2) * 220], t + i * 0.12, 0.1, 0.3, 'square');
  t += 0.8;
  tone([1200, 2400], t, 0.25, 0.3);
  noise(t + 0.3, 2.2, 1.6, 1800);
  for (let i = 0; i < 8; i++) tone([600 + i * 150], t + 0.3 + i * 0.25, 0.08, 0.15, 'sawtooth');
  t += 2.6;
  noise(t, 1.2, 0.5, 1000);
  t += 1.4;
  return { ring, answer, end: t + 0.6 };
}

