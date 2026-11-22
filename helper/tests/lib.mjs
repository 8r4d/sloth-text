// Helpers for the helper's tests: an API client that is either a browser
// (its own session cookie) or the Mac (a bearer token), and the account.

export const URL_BASE = process.env.HELPER_URL;
export const SETUP_PASSWORD = process.env.HELPER_SETUP_PASSWORD;
export const ACCOUNT = { username: 'writer', password: 'writer-password-1' };

let ipCount = 0;

export class Client {
  // `token`: call as the Mac would, with this API token.
  constructor(token = '') {
    this.cookie = '';
    this.token = token;
    ipCount++;
    // Its own IP address (sent as Cloudflare's header), so one test's
    // per-IP login limit doesn't spill into the next.
    this.ip = `10.${(ipCount >> 16) & 255}.${(ipCount >> 8) & 255}.${ipCount & 255}`;
  }

  // { status, body } for api.php?action=…; `query` adds more of the query string.
  async call(action, body, query = '') {
    const headers = { 'Content-Type': 'application/json', 'CF-Connecting-IP': this.ip, Cookie: this.cookie };
    if (this.token) headers.Authorization = `Bearer ${this.token}`;
    const res = await fetch(`${URL_BASE}api.php?action=${action}${query}`, {
      method: body === undefined ? 'GET' : 'POST',
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const set = res.headers.getSetCookie().find(c => c.startsWith('writer_session='));
    if (set) this.cookie = set.split(';')[0];
    const text = await res.text();
    let json = null;
    try { json = JSON.parse(text); } catch {}
    return { status: res.status, body: json ?? text };
  }
}

// A browser signed in to the account (made on the first call).
export async function browser() {
  const c = new Client();
  const me = await c.call('me');
  const r = me.body.needsSetup
    ? await c.call('setup', { setupPassword: SETUP_PASSWORD, ...ACCOUNT })
    : await c.call('login', ACCOUNT);
  if (r.status !== 200) throw new Error(`Sign-in failed: ${JSON.stringify(r.body)}`);
  return c;
}

// The Mac: a client with a fresh API token.
export async function mac() {
  const b = await browser();
  const { body } = await b.call('token_create', { label: 'Test Mac' });
  return new Client(body.token);
}

let projectCount = 0;
// A project id not used by any other test.
export const newProject = () => `proj${Date.now() % 100000}${++projectCount}`;
