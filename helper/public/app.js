// Sloth Writer on the iPad: the scenes marked "Available on iPad" in Sloth
// Writer, to read and write in Safari. Talks to api.php; the Mac syncs with
// the same helper, so the two never talk directly.
//
// Pages (in the address's #):  #/                projects
//                               #/p/ID            a project's scenes
//                               #/p/ID/s/SCENE    writing a scene
//                               #/p/ID/new        a new scene (goes to the Mac's inbox)
//                               #/account         tokens for the Mac, sign out
//
// Writing saves itself a moment after you stop typing, naming the version it
// was edited from. If the Mac changed the scene meanwhile, the helper says so
// and you choose whose words to keep. Until a save goes through (offline,
// say), the words are kept on this iPad as a draft.
'use strict';

const app = document.getElementById('app');

// ----- talking to the helper ------------------------------------------------------

class ApiError extends Error {
  constructor(message, status, data) {
    super(message);
    this.status = status;
    this.data = data;
  }
}

/// A network problem (offline), as opposed to the helper saying no.
const offline = err => !(err instanceof ApiError);

async function api(action, body, query = '', { keepalive = false } = {}) {
  const res = await fetch(`api.php?action=${action}${query}`, {
    method: body === undefined ? 'GET' : 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
    keepalive,
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new ApiError(data.error || `Request failed (${res.status})`, res.status, data);
  return data;
}

const q = (project, scene) => `&p=${encodeURIComponent(project)}` + (scene ? `&id=${encodeURIComponent(scene)}` : '');

// ----- small helpers -----------------------------------------------------------------

const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
const when = ms => (ms ? new Date(ms).toLocaleString() : 'never');
const words = text => (text.replace(/<!--[\s\S]*?(-->|$)/g, ' ').match(/\S+/g) || []).length;

function render(html) {
  app.innerHTML = html;
  window.scrollTo(0, 0);
}

function nav(back) {
  return `<nav class="bar">${back ? `<a href="${back.href}" class="back">‹ ${esc(back.label)}</a>` : '<span></span>'}
    <a href="#/account" class="quiet">Account</a></nav>`;
}

// ----- drafts: words not yet saved to the helper ---------------------------------------

const DRAFTS = 'writer-drafts';

function drafts() {
  try {
    return JSON.parse(localStorage.getItem(DRAFTS)) || {};
  } catch {
    return {};
  }
}

function setDraft(key, draft) {
  const all = drafts();
  if (draft) all[key] = { ...draft, at: Date.now() };
  else delete all[key];
  try {
    localStorage.setItem(DRAFTS, JSON.stringify(all));
  } catch {
    // Storage full or blocked: the words are still on screen.
  }
}

// ----- writing helpers ----------------------------------------------------------------

/// Two hyphens make an em dash, as in Sloth Writer (not inside <!-- notes -->,
/// so their --> still closes them). Smart Punctuation on the iPad often does
/// this already; this covers hardware keyboards and its being off.
function emDashes(textarea) {
  textarea.addEventListener('input', e => {
    if (e.inputType !== 'insertText' || e.data !== '-') return;
    const at = textarea.selectionStart;
    const before = textarea.value.slice(0, at);
    const inNote = before.lastIndexOf('<!--') > before.lastIndexOf('-->');
    if (before.endsWith('--') && !before.endsWith('<!--') && !inNote) {
      textarea.setRangeText('—', at - 2, at, 'end');
      textarea.dispatchEvent(new Event('input'));
    }
  });
}

/// Grow the text area with its text, so the page scrolls rather than the box.
function autoGrow(textarea) {
  const fit = () => {
    textarea.style.height = 'auto';
    textarea.style.height = `${textarea.scrollHeight + 4}px`;
  };
  textarea.addEventListener('input', fit);
  requestAnimationFrame(fit);
}

/// The scene as it would read: italics, bold, headings, quotes, scene breaks;
/// notes left out. Each line is a paragraph, as in Sloth Writer.
function preview(text) {
  const inline = s => esc(s)
    .replace(/\*\*\*(\S(?:.*?\S)?)\*\*\*/g, '<strong><em>$1</em></strong>')
    .replace(/\*\*(\S(?:.*?\S)?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(\S(?:.*?\S)?)\*/g, '<em>$1</em>')
    .replace(/(^|[^\w])_(\S(?:.*?\S)?)_(?!\w)/g, '$1<em>$2</em>');
  return text
    .replace(/<!--[\s\S]*?(-->|$)/g, '')
    .split('\n')
    .map(line => line.trim())
    .filter(Boolean)
    .map(line => {
      if (/^(#|(\*\s*){3,}|(-\s*){3,})$/.test(line)) return '<p class="break">#</p>';
      const heading = line.match(/^(#{1,6})\s+(.*)$/);
      if (heading) return `<h3>${inline(heading[2])}</h3>`;
      if (line.startsWith('>')) return `<blockquote>${inline(line.slice(1).trim())}</blockquote>`;
      return `<p>${inline(line)}</p>`;
    })
    .join('');
}

// ----- pages ---------------------------------------------------------------------------

/// Leaving a page: anything it needs to finish (a scene's last save).
let leaving = null;

async function route() {
  if (leaving) {
    await leaving();
    leaving = null;
  }
  const [kind, project, sub, scene] = location.hash.replace(/^#\/?/, '').split('/');
  try {
    if (kind === 'account') return await showAccount();
    if (kind === 'p' && sub === 's') return await showScene(project, scene);
    if (kind === 'p' && sub === 'new') return showNewScene(project);
    if (kind === 'p') return await showProject(project);
    return await showProjects();
  } catch (err) {
    if (err.status === 401) return start();
    if (offline(err)) return showOffline();
    render(`${nav({ href: '#/', label: 'Projects' })}<p class="error">${esc(err.message)}</p>`);
  }
}

async function showProjects() {
  const projects = await api('projects');
  render(`${nav()}
    <h1>Sloth Writer</h1>
    ${projects.length ? '' : `<p class="quiet">Nothing here yet. In Sloth Writer on your Mac, tick <em>Available on iPad</em>
      in a chapter's inspector, and put this page's address and a token (from <a href="#/account">Account</a>) in its Settings.</p>`}
    <ul class="list">
      ${projects.map(p => `<li><a href="#/p/${encodeURIComponent(p.id)}"><strong>${esc(p.title)}</strong>
        <span class="quiet">${p.scenes} ${p.scenes === 1 ? 'scene' : 'scenes'}</span></a></li>`).join('')}
    </ul>`);
}

async function showProject(project) {
  const [{ title, index }, inbox] = await Promise.all([api('index', undefined, q(project)), api('inbox', undefined, q(project))]);
  const local = drafts();
  // Grouped by the folder each is in, in the binder's order.
  const groups = [];
  for (const scene of index) {
    const last = groups[groups.length - 1];
    if (last && last.chapter === scene.chapter) last.scenes.push(scene);
    else groups.push({ chapter: scene.chapter, scenes: [scene] });
  }
  render(`${nav({ href: '#/', label: 'Projects' })}
    <h1>${esc(title)}</h1>
    ${groups.map(g => `<h2>${esc(g.chapter)}</h2><ul class="list">
      ${g.scenes.map(s => `<li><a href="#/p/${encodeURIComponent(project)}/s/${encodeURIComponent(s.id)}">
        <strong>${esc(s.title)}</strong>
        ${s.synopsis ? `<span class="synopsis">${esc(s.synopsis)}</span>` : ''}
        <span class="quiet">${[s.status, `${s.words} words`].filter(Boolean).map(esc).join(' · ')}
          ${local[`${project}/${s.id}`] ? ' · <span class="unsaved">not saved yet</span>' : ''}</span></a></li>`).join('')}
    </ul>`).join('')}
    ${index.length ? '' : '<p class="quiet">No scenes are on the iPad for this project.</p>'}
    <p><a class="button" href="#/p/${encodeURIComponent(project)}/new">New Scene</a></p>
    ${inbox.length ? `<p class="quiet">${inbox.length} new ${inbox.length === 1 ? 'scene is' : 'scenes are'}
      waiting for your Mac to take into the Manuscript.</p>` : ''}`);
}

async function showScene(project, sceneId) {
  const key = `${project}/${sceneId}`;
  let info = { title: 'Scene' };
  let scene;
  try {
    const { index } = await api('index', undefined, q(project));
    info = index.find(s => s.id === sceneId) || info;
    scene = await api('scene', undefined, q(project, sceneId));
  } catch (err) {
    if (!offline(err)) {
      if (err.status === 404) {
        render(`${nav({ href: `#/p/${encodeURIComponent(project)}`, label: 'Back' })}<h1>${esc(info.title)}</h1>
          <p class="quiet">This scene hasn't reached the helper yet: Sloth Writer sends it at its next sync.</p>`);
        return;
      }
      throw err;
    }
  }
  const draft = drafts()[key];
  if (!scene && !draft) throw new TypeError('offline');

  // Where to start: the helper's text, unless this iPad has unsaved words.
  let base = scene ? scene.version : draft.base;
  let saved = scene ? scene.text : draft.text;
  let text = draft && (!scene || draft.base === scene.version || draft.text !== scene.text) ? draft.text : saved;
  const title = info.title || draft?.title || 'Scene';

  render(`${nav({ href: `#/p/${encodeURIComponent(project)}`, label: 'Back' })}
    <header class="scene-head">
      <h1>${esc(title)}</h1>
      <div class="tools"><span id="state" class="quiet"></span>
        <span id="count" class="quiet"></span>
        <button id="toggle">Preview</button></div>
    </header>
    <div id="clash" class="clash" hidden></div>
    <textarea id="text" spellcheck="true" autocapitalize="sentences" aria-label="${esc(title)}"></textarea>
    <article id="preview" class="preview" hidden></article>`);
  const textarea = document.getElementById('text');
  const stateEl = document.getElementById('state');
  const countEl = document.getElementById('count');
  textarea.value = text;
  emDashes(textarea);
  autoGrow(textarea);

  const show = state => (stateEl.textContent = state);
  const count = () => (countEl.textContent = `${words(textarea.value)} words`);
  count();

  let timer = null;
  let saving = false;
  let clash = null;

  async function save({ keepalive = false } = {}) {
    clearTimeout(timer);
    if (saving || clash || textarea.value === saved) {
      if (textarea.value === saved && !clash) show('Saved');
      return;
    }
    saving = true;
    const sending = textarea.value;
    show('Saving…');
    try {
      const r = await api('scene_put', { text: sending, base }, q(project, sceneId), { keepalive });
      base = r.version;
      saved = sending;
      if (textarea.value === saved) {
        setDraft(key, null);
        show('Saved');
      } else {
        schedule();
      }
    } catch (err) {
      if (err.status === 409) {
        showClash(err.data.current);
      } else if (offline(err)) {
        show('Offline: kept on this iPad');
      } else {
        show(err.message);
      }
    } finally {
      saving = false;
    }
  }

  function schedule() {
    clearTimeout(timer);
    timer = setTimeout(save, 1500);
  }

  // The Mac changed the scene while this iPad was writing it.
  function showClash(current) {
    clash = current;
    show('Changed on your Mac');
    const box = document.getElementById('clash');
    box.hidden = false;
    box.innerHTML = `<p><strong>Your Mac changed this scene</strong> while you were writing here.</p>
      <p class="quiet">Keep yours to replace the Mac's (its words stay in the scene's history), or use the Mac's
      (yours are saved as a new scene, for you to sort out in Sloth Writer).</p>
      <button id="keep-mine" class="primary">Keep Mine</button> <button id="use-macs">Use the Mac's</button>`;
    document.getElementById('keep-mine').onclick = () => {
      base = clash ? clash.version : base;
      clash = null;
      box.hidden = true;
      save();
    };
    document.getElementById('use-macs').onclick = async () => {
      const mine = textarea.value;
      try {
        await api('inbox_add', { title: `${title} (iPad version)`, text: mine }, q(project));
      } catch (err) {
        show(err.message);
        return;
      }
      textarea.value = clash ? clash.text : '';
      base = clash ? clash.version : base;
      saved = textarea.value;
      clash = null;
      box.hidden = true;
      setDraft(key, null);
      textarea.dispatchEvent(new Event('input'));
      show('Saved');
    };
  }

  textarea.addEventListener('input', () => {
    count();
    if (textarea.value !== saved) {
      setDraft(key, { text: textarea.value, base, title });
      show('Not saved yet');
      schedule();
    }
  });

  document.getElementById('toggle').onclick = () => {
    const previewEl = document.getElementById('preview');
    const showing = !previewEl.hidden;
    previewEl.hidden = showing;
    textarea.hidden = !showing;
    document.getElementById('toggle').textContent = showing ? 'Preview' : 'Write';
    if (!showing) previewEl.innerHTML = preview(textarea.value);
  };

  // Back online, or every so often: send what's waiting.
  const retry = () => textarea.value !== saved && !clash && save();
  window.addEventListener('online', retry);
  const every = setInterval(retry, 20000);
  // Leaving the page or the app: save what's typed (keepalive lets it
  // finish even as Safari puts the page away).
  const flush = () => document.visibilityState === 'hidden' && save({ keepalive: true });
  document.addEventListener('visibilitychange', flush);
  leaving = async () => {
    await save();
    clearInterval(every);
    window.removeEventListener('online', retry);
    document.removeEventListener('visibilitychange', flush);
  };

  if (text !== saved) {
    show('Not saved yet');
    // A draft from before the Mac's last change: that's a clash.
    if (scene && draft && draft.base !== scene.version) showClash({ text: scene.text, version: scene.version });
    else schedule();
  } else {
    show('Saved');
  }
}

function showNewScene(project) {
  const key = `${project}/new`;
  const draft = drafts()[key] || { title: '', text: '' };
  render(`${nav({ href: `#/p/${encodeURIComponent(project)}`, label: 'Back' })}
    <h1>New Scene</h1>
    <p class="quiet">It goes to your Mac, which puts it in the Manuscript at its next sync.</p>
    <input id="title" placeholder="Title" value="${esc(draft.title)}">
    <textarea id="text" placeholder="Write…" aria-label="New scene"></textarea>
    <p class="error" id="error"></p>
    <button id="send" class="primary">Send to My Mac</button>`);
  const textarea = document.getElementById('text');
  const titleEl = document.getElementById('title');
  textarea.value = draft.text;
  emDashes(textarea);
  autoGrow(textarea);
  const keep = () => setDraft(key, { title: titleEl.value, text: textarea.value });
  textarea.addEventListener('input', keep);
  titleEl.addEventListener('input', keep);
  document.getElementById('send').onclick = async () => {
    try {
      await api('inbox_add', { title: titleEl.value, text: textarea.value }, q(project));
      setDraft(key, null);
      location.hash = `#/p/${encodeURIComponent(project)}`;
    } catch (err) {
      document.getElementById('error').textContent = offline(err) ? 'Offline: kept on this iPad; send it when you\'re back online.' : err.message;
    }
  };
}

/// No connection: the scenes with unsaved words can still be opened.
function showOffline() {
  const waiting = Object.entries(drafts()).filter(([key]) => !key.endsWith('/new'));
  render(`<h1>Sloth Writer</h1><p class="quiet">Offline. Scenes you were writing here can still be opened;
    they're saved when you're back online.</p>
    <ul class="list">${waiting.map(([key, d]) => {
      const [project, scene] = key.split('/');
      return `<li><a href="#/p/${encodeURIComponent(project)}/s/${encodeURIComponent(scene)}"><strong>${esc(d.title)}</strong></a></li>`;
    }).join('')}</ul>`);
}

// ----- signing in, and the account -----------------------------------------------------

function form(fields, button, onSubmit) {
  render(`<h1>Sloth Writer</h1>
    <form>
      ${fields.map(([name, label, type]) => `<label for="${name}">${label}</label><input id="${name}" name="${name}" type="${type}" required>`).join('')}
      <p class="error" id="error"></p>
      <button class="primary">${button}</button>
    </form>`);
  app.querySelector('form').addEventListener('submit', async e => {
    e.preventDefault();
    try {
      await onSubmit(Object.fromEntries(new FormData(e.target)));
    } catch (err) {
      app.querySelector('#error').textContent = err.message;
    }
  });
}

function showSetup() {
  form(
    [['setupPassword', 'Setup password (from writer-config.php)', 'password'], ['username', 'Choose a username', 'text'], ['password', 'Choose a password (8 or more characters)', 'password']],
    'Create My Account',
    async v => { await api('setup', v); location.hash = '#/account'; route(); },
  );
}

function showSignIn() {
  form([['username', 'Username', 'text'], ['password', 'Password', 'password']], 'Sign In', async v => {
    await api('login', v);
    route();
  });
}

async function showAccount(newToken) {
  const tokens = await api('tokens');
  render(`${nav({ href: '#/', label: 'Projects' })}
    <h1>Account</h1>
    <p class="quiet">Sloth Writer on your Mac syncs with this helper using a token.
    Make one here, then paste it into Writer's Settings along with this page's address.</p>
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
    showAccount(token);
  });
  app.querySelectorAll('[data-revoke]').forEach(b => b.addEventListener('click', async () => {
    if (confirm('Revoke this token? Sloth Writer using it will stop syncing.')) {
      await api('token_delete', {}, `&id=${b.dataset.revoke}`);
      showAccount();
    }
  }));
  app.querySelector('#signout').addEventListener('click', async () => {
    await api('logout', {});
    showSignIn();
  });
}

async function start() {
  try {
    const me = await api('me');
    if (me.user) route();
    else if (me.needsSetup) showSetup();
    else showSignIn();
  } catch (err) {
    if (offline(err)) showOffline();
    else render(`<h1>Sloth Writer</h1><p class="error">${esc(err.message)}</p>`);
  }
}

window.addEventListener('hashchange', route);
// index.php names the worker with its files' versions (see sw.js).
if ('serviceWorker' in navigator) navigator.serviceWorker.register(window.WORKER || 'sw.js').catch(() => {});
start();
