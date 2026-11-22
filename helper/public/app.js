// The helper's page. For now: first-time setup, signing in, and the tokens
// Sloth Writer on the Mac uses to sync. (The scene editor for the iPad comes
// next, in this same page.)
'use strict';

const app = document.getElementById('app');

async function api(action, body, query = '') {
  const res = await fetch(`api.php?action=${action}${query}`, {
    method: body === undefined ? 'GET' : 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || `Request failed (${res.status})`);
  return data;
}

const esc = s => String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
const when = ms => (ms ? new Date(ms).toLocaleString() : 'never');

function render(html) {
  app.innerHTML = `<h1>Sloth Writer</h1>${html}`;
}

function form(fields, button, onSubmit) {
  render(`
    <form>
      ${fields.map(([name, label, type]) => `<label for="${name}">${label}</label><input id="${name}" name="${name}" type="${type}" required>`).join('')}
      <p class="error" id="error"></p>
      <button class="primary">${button}</button>
    </form>`);
  app.querySelector('form').addEventListener('submit', async e => {
    e.preventDefault();
    const values = Object.fromEntries(new FormData(e.target));
    try {
      await onSubmit(values);
    } catch (err) {
      app.querySelector('#error').textContent = err.message;
    }
  });
}

function showSetup() {
  form(
    [['setupPassword', 'Setup password (from writer-config.php)', 'password'], ['username', 'Choose a username', 'text'], ['password', 'Choose a password (8 or more characters)', 'password']],
    'Create My Account',
    async v => { await api('setup', v); showTokens(); },
  );
}

function showSignIn() {
  form([['username', 'Username', 'text'], ['password', 'Password', 'password']], 'Sign In', async v => {
    await api('login', v);
    showTokens();
  });
}

async function showTokens(newToken) {
  const tokens = await api('tokens');
  render(`
    <p class="quiet">Sloth Writer on your Mac syncs with this helper using a token.
    Make one here, then paste it into Writer's settings along with this page's address.</p>
    ${newToken ? `<h2>Your new token</h2><p>Copy it now: it won't be shown again.</p><p class="token">${esc(newToken)}</p>` : ''}
    <h2>Tokens</h2>
    <ul class="tokens">
      ${tokens.map(t => `<li><span>${esc(t.label)}<br><span class="quiet">made ${when(t.createdAt)}, last used ${when(t.lastUsedAt)}</span></span>
        <button data-revoke="${t.id}">Revoke</button></li>`).join('') || '<li class="quiet">None yet.</li>'}
    </ul>
    <form id="make"><label for="label">Name for a new token</label><input id="label" name="label" value="My Mac">
    <button class="primary">Make a Token</button></form>
    <p><button id="signout">Sign Out</button></p>`);
  app.querySelector('#make').addEventListener('submit', async e => {
    e.preventDefault();
    const { token } = await api('token_create', { label: e.target.label.value });
    showTokens(token);
  });
  app.querySelectorAll('[data-revoke]').forEach(b => b.addEventListener('click', async () => {
    if (confirm('Revoke this token? Sloth Writer using it will stop syncing.')) {
      await api('token_delete', {}, `&id=${b.dataset.revoke}`);
      showTokens();
    }
  }));
  app.querySelector('#signout').addEventListener('click', async () => {
    await api('logout', {});
    showSignIn();
  });
}

(async () => {
  try {
    const me = await api('me');
    if (me.user) showTokens();
    else if (me.needsSetup) showSetup();
    else showSignIn();
  } catch (err) {
    render(`<p class="error">${esc(err.message)}</p>`);
  }
})();
