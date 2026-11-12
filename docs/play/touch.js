// touch - an on-screen joystick for phones and tablets, drawn in breadbin's own
// page around the emulator rather than over the C64 screen: the <blip-deck> control deck
// (github.com/jacobandresen/blip-deck, MIT; loaded from jsDelivr by index.html), a
// ball-top arcade stick with an 8-way gate that leans with the thumb, and red
// FIRE caps. A row of keys covers what joystick games ask for at the title
// screen (SPACE, RETURN, F1-F7, RUN/STOP) and the Y/N and number questions of
// crack trainer menus. Input reaches vc64web through the listener installed by
// PAD_LISTENER.

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

/**
 * Build the pad inside `root`: the keys, and a <blip-deck> (from jsDelivr, see index.html) for the stick and FIRE.
 * `send(message)` posts to the emulator frame. Returns { el, setPort(n), destroy() }.
 */
export function createPad(root, send, { port = 2 } = {}) {
  const { BlipDeck, BlipController } = window;
  const el = document.createElement('div');
  el.className = 'pad';
  el.innerHTML = '<div class="pad-keys"></div><div id="topbar" class="kiosk-bar"><blip-deck seats="1" buttons=\'[{"label":"FIRE"}]\'></blip-deck></div>';
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

  // The deck reads its layout from the page: upright, or sideways on a phone.
  const html = document.documentElement;
  const layout = () => html.setAttribute('data-layout', innerHeight <= 520 && innerWidth > innerHeight * 1.25 ? 'landscape' : 'upright');
  layout();
  addEventListener('resize', layout);
  html.setAttribute('data-controls', 'stick');
  html.toggleAttribute('data-has-touch', 'ontouchstart' in window || navigator.maxTouchPoints > 0);
  const stick = el.querySelector('#stick-base'), fire = el.querySelector('#fire-buttons');

  // The deck's logical inputs become the C64 joystick's: both caps fire.
  const held = {};
  let state = { x: 'RELEASE_X', y: 'RELEASE_Y' };
  BlipController.init({
    onInput(name, down) {
      held[name] = down;
      if (name === 'button1' || name === 'button2') {
        send({ bbjoy: [port + (down ? 'PRESS_FIRE' : 'RELEASE_FIRE')] });
        if (down) buzz(10);
        return;
      }
      BlipDeck.lean(stick, (held.right ? 1 : 0) - (held.left ? 1 : 0), (held.down ? 1 : 0) - (held.up ? 1 : 0));
      const x = held.right ? 'PULL_RIGHT' : held.left ? 'PULL_LEFT' : 'RELEASE_X';
      const y = held.down ? 'PULL_DOWN' : held.up ? 'PULL_UP' : 'RELEASE_Y';
      const cmds = [];
      if (x !== state.x) cmds.push(port + x);
      if (y !== state.y) cmds.push(port + y);
      state = { x, y };
      if (cmds.length) { send({ bbjoy: cmds }); if (x !== 'RELEASE_X' || y !== 'RELEASE_Y') buzz(4); }
    },
  });
  BlipController.bindButtons(fire);
  const unbind = BlipDeck.bindStick({ stick, fire, dirs: { up: 'up', down: 'down', left: 'left', right: 'right' } });

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
    destroy: () => {
      unbind();
      BlipController.releaseAll();
      removeEventListener('resize', layout);
      for (const a of ['data-layout', 'data-controls', 'data-has-touch']) html.removeAttribute(a);
      el.remove();
    },
  };
}

function buzz(ms) {
  try {
    navigator.vibrate?.(ms);
  } catch {}
}
