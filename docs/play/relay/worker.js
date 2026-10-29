// A CSDb relay for breadbin's browser version, as a Cloudflare Worker (the
// free plan is plenty). csdb.dk only sends CORS headers to sites it has
// allowlisted; this forwards GET requests for csdb.dk and adds them, so the page
// can read toplists and release details and download demo files.
//
// Deploy: `npx wrangler deploy relay/worker.js --name breadbin-csdb`, then in
// breadbin's Settings set the relay to
//   https://breadbin-csdb.<your-account>.workers.dev/?url={url}
//
// Set ALLOWED_ORIGIN to the site that may use it (your GitHub Pages origin).

const ALLOWED_ORIGIN = 'https://jacobandresen.github.io';
const ALLOWED_HOSTS = ['csdb.dk'];

export default {
  async fetch(request) {
    const cors = {
      'access-control-allow-origin': ALLOWED_ORIGIN,
      'access-control-allow-methods': 'GET, OPTIONS',
      'access-control-max-age': '86400',
    };
    if (request.method === 'OPTIONS') return new Response(null, { headers: cors });
    if (request.method !== 'GET') return new Response('GET only', { status: 405, headers: cors });

    const target = new URL(request.url).searchParams.get('url') || '';
    let url;
    try {
      url = new URL(target);
    } catch {
      return new Response('missing ?url=', { status: 400, headers: cors });
    }
    if (url.protocol !== 'https:' || !ALLOWED_HOSTS.includes(url.hostname)) {
      return new Response('host not allowed', { status: 403, headers: cors });
    }

    const upstream = await fetch(url, {
      headers: { 'user-agent': 'breadbin-relay/1.0 (+https://github.com/jacobandresen/breadbin)' },
      cf: { cacheTtl: 3600, cacheEverything: true },
    });
    const headers = new Headers(cors);
    for (const h of ['content-type', 'content-length', 'content-disposition', 'last-modified']) {
      if (upstream.headers.has(h)) headers.set(h, upstream.headers.get(h));
    }
    return new Response(upstream.body, { status: upstream.status, headers });
  },
};
