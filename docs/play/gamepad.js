// gamepad - any connected gamepad as the C64 joystick: stick or d-pad to move,
// the face buttons to fire. vc64web only offers gamepads through its own menu,
// so breadbin polls them and sends joystick commands to the emulator frame.

const DEAD = 0.4;

/** Poll gamepads, sending { bbjoy } messages for `port()`. Returns stop(). */
export function pollGamepads(send, port) {
  let last = { x: 'RELEASE_X', y: 'RELEASE_Y', fire: false };
  let raf = 0;
  const tick = () => {
    raf = requestAnimationFrame(tick);
    const pad = [...(navigator.getGamepads?.() || [])].find(Boolean);
    if (!pad) return;
    const b = (i) => !!pad.buttons[i]?.pressed;
    const [ax = 0, ay = 0] = pad.axes;
    const now = {
      x: ax > DEAD || b(15) ? 'PULL_RIGHT' : ax < -DEAD || b(14) ? 'PULL_LEFT' : 'RELEASE_X',
      y: ay > DEAD || b(13) ? 'PULL_DOWN' : ay < -DEAD || b(12) ? 'PULL_UP' : 'RELEASE_Y',
      fire: b(0) || b(1) || b(2) || b(3),
    };
    const n = port();
    const cmds = [];
    if (now.x !== last.x) cmds.push(n + now.x);
    if (now.y !== last.y) cmds.push(n + now.y);
    if (now.fire !== last.fire) cmds.push(n + (now.fire ? 'PRESS_FIRE' : 'RELEASE_FIRE'));
    last = now;
    if (cmds.length) send({ bbjoy: cmds });
  };
  tick();
  return () => cancelAnimationFrame(raf);
}
