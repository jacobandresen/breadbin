// hack - the Hack tab: the Studio (studio/studio.js), a small game studio
// for the C64: C (cc65) or assembly (ACME), sprites, tiles,
// rooms and SID music, saved in the browser or to a folder on your disk,
// running on the C64 beside the code.
//
// The Studio (with the code editor, half a megabyte) is fetched the first
// time the tab opens, not with the rest of the page.

import { $, h } from './ui.js';

export async function initHack() {
  const root = $('#hack');
  root.replaceChildren(h('p.studio-loading', {}, 'Opening the Studio…'));
  try {
    const { initStudio } = await import('./studio/studio.js');
    return await initStudio(root);
  } catch (e) {
    root.replaceChildren(h('p.studio-loading', {}, `The Studio didn't open: ${e.message}`));
    throw e;
  }
}
