// examples - the projects the Studio starts from. The C ones live next to
// the Studio (examples/<id>/, listed in its files.json); the assembly ones
// are classic intros fetched from their authors' GitHub repositories, as the
// Hack tab has always had them.

import { resource } from './resource.js';
import * as store from '../store.js';
import { pretty } from './project.js';

const raw = (repo, sha, dir = '') => `https://raw.githubusercontent.com/${repo}/${sha}/${dir}`;

export const EXAMPLES = [
  {
    id: 'bubbler', title: 'Bubbler', lang: 'c', icon: '◍', color: 'linear-gradient(135deg, #5fd3a6, #2a7f62)', template: true,
    about: 'A one-screen platformer: blow bubbles, trap the monsters, pop them for points.',
  },
  {
    id: 'bouncer', title: 'Bouncer', lang: 'c', icon: '▬', color: 'linear-gradient(135deg, #ff8a5c, #c4622d)', template: true,
    about: 'Bricks, a bat and a ball: catch the falling pickups.',
  },
  {
    id: 'pixels', title: 'Pixels', lang: 'c', icon: '◐', color: 'linear-gradient(135deg, #d3869b, #6f3d86)', template: true,
    about: 'The bitmap mode: lines, circles, blits racing plots, bouncing balls, peek and poke.',
  },
  {
    id: 'blank', title: 'Empty game', lang: 'c', icon: '✦', color: 'linear-gradient(135deg, #7ab8ff, #4a4fb0)', template: true,
    about: 'A sprite you can steer and a room to walk in: the smallest start.',
  },
  {
    id: 'fairlight', title: 'Fairlight intro', lang: 'acme', icon: '❖', color: 'linear-gradient(135deg, #b8acff, #5b4b9a)',
    about: 'Fairlight\'s classic intro, disassembled and cleaned up by awsm.',
    root: raw('esshahn/c64-fairlight', '5531171c707803bb68a97389521910442f52241f'), entry: 'code/main.asm', url: 'https://github.com/esshahn/c64-fairlight',
  },
  {
    id: 'eaglesoft', title: 'Eaglesoft intro', lang: 'acme', icon: '❖', color: 'linear-gradient(135deg, #ffd36b, #b8862d)',
    about: 'The Eaglesoft Incorporated intro, disassembled by awsm.',
    root: raw('esshahn/c64-eaglesoft', '6b5dedb05db899831d768b373702e06f94cecde1'), entry: 'code/main.asm', url: 'https://github.com/esshahn/c64-eaglesoft',
  },
  {
    id: 'haz', title: 'Can I Haz Member', lang: 'acme', icon: '❖', color: 'linear-gradient(135deg, #ff5d73, #8a2a4a)',
    about: 'A colour-wash intro by awsm of Mayday!, music by Laxity.',
    root: raw('esshahn/c64-prods-by-awsm', 'aa95e1e9b4145839445cf39aa74259e1a7d0f1dd', 'can-i-haz-member/'), entry: 'index.asm', url: 'https://github.com/esshahn/c64-prods-by-awsm',
  },
];

const INCLUDE = /^[ \t]*!(source|src|bin|binary)[ \t]+"([^"]+)"/gim;
const isAsmText = (path) => /\.(asm|a|s|inc|txt)$/i.test(path);

/** The files of example `ex`: path -> text (or bytes, for an intro's graphics and music). */
export async function exampleFiles(ex) {
  if (ex.root) return fetchIntro(ex);
  const list = JSON.parse(new TextDecoder().decode(await resource(`examples/${ex.id}/files.json`)));
  const files = {};
  await Promise.all(list.map(async (p) => (files[p] = new TextDecoder().decode(await resource(`examples/${ex.id}/${p}`)))));
  return files;
}

/** An ACME intro: its entry file and everything it includes, from GitHub. */
async function fetchIntro(ex) {
  const files = {};
  const todo = [ex.entry];
  while (todo.length) {
    const path = todo.shift();
    if (path in files) continue;
    const bytes = await store.cachedBytes(ex.root + path);
    files[path] = isAsmText(path) ? new TextDecoder('latin1').decode(bytes) : bytes;
    if (isAsmText(path)) for (const [, , inc] of files[path].matchAll(INCLUDE)) todo.push(inc);
  }
  files['project.json'] = `${pretty({ name: ex.title, lang: 'acme', entry: ex.entry, about: ex.about, source: ex.url })}\n`;
  files['README.md'] = `# ${ex.title}\n\n${ex.about}\n\nSource: ${ex.url}\n\nAssembled with ACME (\`acme -f cbm -o intro.prg ${ex.entry}\`).\n`;
  return files;
}
