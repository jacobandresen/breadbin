// hvsc - SID files from the High Voltage SID Collection at hvsc.de, which sends
// CORS headers. Verified tunes know their HVSC path.

import { cachedBytes } from './store.js';

/** The SID at an HVSC path ("/MUSICIANS/H/Hubbard_Rob/Commando.sid"), cached for good. */
export const sid = (path) => cachedBytes(`https://www.hvsc.de/download/C64Music${path.split('/').map(encodeURIComponent).join('/')}`);
