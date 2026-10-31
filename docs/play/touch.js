// touch - an on-screen joystick for phones and tablets, drawn in breadbin's own
// page around the emulator rather than over the C64 screen: the control deck of
// blip (github.com/jacobandresen/blip, MIT), a ball-top arcade stick that leans
// with the thumb and a red arcade FIRE button. The stick floats (it centres
// wherever your thumb lands in its half), and a row of keys covers what joystick games ask for at the
// title screen (SPACE, RETURN, F1-F7, RUN/STOP) and the Y/N and number
// questions of crack trainer menus. Input reaches vc64web through
// the listener installed by PAD_LISTENER.

/** Script for vc64web's {cmd: "script"}: route breadbin's pad messages to the
 *  emulator's own joystick and keyboard handlers. */
export const PAD_LISTENER = `
  if (!window.breadbinPad) {
    window.breadbinPad = true;
    // Space is the C64's SPACE key and joystick fire at once: crack intros
    // and title screens ask for SPACE, games for fire
    delete joystick_keydown_map.Space;
    delete joystick_keyup_map.Space;
    const fire = (down) => (e) => e.code === 'Space' && !e.repeat && emit_joystick_cmd((port1 === 'keys' ? '1' : '2') + (down ? 'PRESS_FIRE' : 'RELEASE_FIRE'));
    addEventListener('keydown', fire(true), true);
    addEventListener('keyup', fire(false), true);
    addEventListener('message', (e) => {
      const d = e.data;
      if (!d || typeof d !== 'object') return;
      if (d.bbjoy) for (const c of d.bbjoy) emit_joystick_cmd(c);
      if (d.bbkey) wasm_key(d.bbkey[0], d.bbkey[1], d.bbkey[2]);
      // the port the cursor keys + space drive
      if (d.bbport) [port1, port2] = d.bbport === 1 ? ['keys', 'none'] : ['none', 'keys'];
    });
  }`;

// C64 keyboard matrix positions [row, column].
const KEYS = [
  ['SPACE', [7, 4]],
  ['RETURN', [0, 1]],
  ['F1', [0, 4]],
  ['F3', [0, 5]],
  ['F5', [0, 6]],
  ['F7', [0, 3]],
  ['RUN/STOP', [7, 7]],
  ['Y', [3, 1]],
  ['N', [4, 7]],
  ['0', [4, 3]],
  ['1', [7, 0]],
];

const DEAD_ZONE = 14; // px before a direction registers
const REACH = 46; // px the knob can travel

/**
 * Build the pad inside `root`. `send(message)` posts to the emulator frame.
 * Returns { el, setPort(n), destroy() }.
 */
export function createPad(root, send, { port = 2 } = {}) {
  const el = document.createElement('div');
  el.className = 'pad';
  el.innerHTML = `
    <div class="pad-keys"></div>
    <div class="pad-stick" aria-label="Joystick">
      <div class="deck-stick">
        <div class="deck-boot"></div>
        <div class="deck-handle"><div class="deck-shaft"></div><div class="deck-ball"></div></div>
      </div>
    </div>
    <button class="pad-fire arcade-btn" aria-label="Fire"><span class="arcade-btn-cap"></span></button>`;
  root.append(el);

  const keys = el.querySelector('.pad-keys');
  for (const [label, [row, col]] of KEYS) {
    const b = document.createElement('button');
    b.className = 'key pill pad-key';
    b.textContent = label;
    const press = (down) => (e) => {
      e.preventDefault();
      b.classList.toggle('on', down);
      send({ bbkey: [row, col, down ? 1 : 0] });
      if (down) buzz(6);
    };
    b.addEventListener('pointerdown', press(true));
    for (const ev of ['pointerup', 'pointercancel', 'pointerleave']) b.addEventListener(ev, press(false));
    keys.append(b);
  }

  // ── stick ──
  const zone = el.querySelector('.pad-stick');
  const stick = el.querySelector('.deck-stick');
  // the stick leans toward the thumb: --dx / --dy run from -1 to 1
  const lean = (dx = 0, dy = 0) => {
    stick.style.setProperty('--dx', (dx / REACH).toFixed(2));
    stick.style.setProperty('--dy', (dy / REACH).toFixed(2));
  };
  let stickId = null;
  let origin = null;
  let state = { x: 'RELEASE_X', y: 'RELEASE_Y' };

  const setDir = (x, y) => {
    const cmds = [];
    if (x !== state.x) cmds.push(port + x);
    if (y !== state.y) cmds.push(port + y);
    if (!cmds.length) return;
    state = { x, y };
    send({ bbjoy: cmds });
    if (x !== 'RELEASE_X' || y !== 'RELEASE_Y') buzz(4);
  };

  zone.addEventListener('pointerdown', (e) => {
    if (stickId !== null) return;
    e.preventDefault();
    stickId = e.pointerId;
    zone.setPointerCapture(e.pointerId);
    origin = { x: e.clientX, y: e.clientY };
    zone.classList.add('active');
    move(e);
  });
  const move = (e) => {
    if (e.pointerId !== stickId) return;
    let dx = e.clientX - origin.x;
    let dy = e.clientY - origin.y;
    const dist = Math.hypot(dx, dy);
    if (dist > REACH) {
      // the centre follows a wandering thumb, so turning back acts at once
      const pull = dist - REACH;
      origin.x += (dx / dist) * pull;
      origin.y += (dy / dist) * pull;
      dx = e.clientX - origin.x;
      dy = e.clientY - origin.y;
    }
    lean(dx, dy);
    if (Math.hypot(dx, dy) < DEAD_ZONE) return setDir('RELEASE_X', 'RELEASE_Y');
    // eight 45° sectors, like a real microswitch stick
    const sector = Math.round(Math.atan2(dy, dx) / (Math.PI / 4));
    const x = [0, 1, -1].includes(sector) ? 'PULL_RIGHT' : [3, 4, -4, -3].includes(sector) ? 'PULL_LEFT' : 'RELEASE_X';
    const y = [1, 2, 3].includes(sector) ? 'PULL_DOWN' : [-1, -2, -3].includes(sector) ? 'PULL_UP' : 'RELEASE_Y';
    setDir(x, y);
  };
  zone.addEventListener('pointermove', move);
  const release = (e) => {
    if (e.pointerId !== stickId) return;
    stickId = null;
    zone.classList.remove('active');
    lean();
    setDir('RELEASE_X', 'RELEASE_Y');
  };
  zone.addEventListener('pointerup', release);
  zone.addEventListener('pointercancel', release);

  // ── fire ──
  const fire = el.querySelector('.pad-fire');
  const fireDown = (down) => (e) => {
    e.preventDefault();
    if (fire.classList.contains('on') === down) return;
    fire.classList.toggle('on', down);
    send({ bbjoy: [port + (down ? 'PRESS_FIRE' : 'RELEASE_FIRE')] });
    if (down) buzz(10);
  };
  fire.addEventListener('pointerdown', fireDown(true));
  for (const ev of ['pointerup', 'pointercancel', 'pointerleave']) fire.addEventListener(ev, fireDown(false));

  // the pad never scrolls, zooms or opens a context menu
  el.addEventListener('contextmenu', (e) => e.preventDefault());

  const setPort = (n) => {
    send({ bbjoy: [`${port}RELEASE_XY`, `${port}RELEASE_FIRE`] });
    state = { x: 'RELEASE_X', y: 'RELEASE_Y' };
    port = n;
  };

  return {
    el,
    setPort,
    destroy: () => el.remove(),
  };
}

function buzz(ms) {
  try {
    navigator.vibrate?.(ms);
  } catch {}
}
