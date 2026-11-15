// touch - an on-screen joystick for phones and tablets, drawn in breadbin's own
// page around the emulator rather than over the C64 screen: the <blip-deck> control
// deck (github.com/jacobandresen/blip-deck, MIT; loaded from jsDelivr by index.html),
// a ball-top arcade stick and one FIRE button, as a C64 joystick has. A row of keys
// covers what joystick games ask for at the title screen (SPACE, RETURN, F1-F7,
// RUN/STOP) and the Y/N and number questions of crack trainer menus. Input reaches
// vc64web through the listener installed by PAD_LISTENER.

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

/** Sideways on a phone: the deck's controls go to the gutters beside the screen. */
const LANDSCAPE = matchMedia('(orientation: landscape) and (max-height: 600px)');

/**
 * Build the pad inside `root`: the keys, and a <blip-deck> for the stick and FIRE.
 * `send(message)` posts to the emulator frame. Returns { el, setPort(n), destroy() }.
 */
export function createPad(root, send, { port = 2 } = {}) {
  const el = document.createElement('div');
  el.className = 'pad';
  el.innerHTML = '<div class="pad-keys"></div><blip-deck buttons=\'["FIRE"]\'></blip-deck>';
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

  // The deck's names become the C64 joystick's commands, sent for an axis when it changes: it is released only when
  // neither of its ways is held.
  const deck = el.querySelector('blip-deck');
  const held = new Set();
  let sent = { X: 'RELEASE_X', Y: 'RELEASE_Y' };
  const axis = (minus, plus, x) => (held.has(plus) ? `PULL_${plus.toUpperCase()}` : held.has(minus) ? `PULL_${minus.toUpperCase()}` : `RELEASE_${x}`);
  deck.addEventListener('blip-input', ({ detail: { name, down } }) => {
    if (name === 'button1') {
      send({ bbjoy: [port + (down ? 'PRESS_FIRE' : 'RELEASE_FIRE')] });
      if (down) buzz(10);
      return;
    }
    if (down) held.add(name);
    else held.delete(name);
    const now = { X: axis('left', 'right', 'X'), Y: axis('up', 'down', 'Y') };
    send({ bbjoy: ['X', 'Y'].filter((a) => now[a] !== sent[a]).map((a) => port + now[a]) });
    sent = now;
    if (down) buzz(4);
  });
  const layout = () => deck.setAttribute('layout', LANDSCAPE.matches ? 'landscape' : 'upright');
  layout();
  LANDSCAPE.addEventListener('change', layout);

  const setPort = (n) => {
    deck.release(); // its releases go to the old port
    port = n;
  };

  return {
    el,
    setPort,
    destroy: () => {
      deck.release();
      LANDSCAPE.removeEventListener('change', layout);
      el.remove();
    },
  };
}

function buzz(ms) {
  try {
    navigator.vibrate?.(ms);
  } catch {}
}
