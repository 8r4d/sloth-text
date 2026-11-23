// Sloth Writer on the iPad: the scenes marked "Available on iPad" in Sloth
// Writer, to read and write in Safari. Talks to api.php; the Mac syncs with
// the same helper, so the two never talk directly.
//
// Pages (in the address's #):  #/                projects
//                               #/p/ID            a project's scenes by chapter (folding, finding,
//                                                 words against targets, today against the goal)
//                               #/p/ID/s/SCENE    writing a scene, with its notes above it
//                               #/p/ID/new        a new scene (goes to the Mac's inbox)
//                               #/account         tokens for the Mac, password, sign out
//
// Signed out: Sign In, with Forgot Password? (the setup password from
// writer-config.php unlocks the account), or Create My Account on the first
// visit. The forms tell password managers which field is which, so they save
// the account's password, never the setup password.
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
const fmt = n => Number(n || 0).toLocaleString();
const plural = (n, one, many) => `${fmt(n)} ${n === 1 ? one : many}`;

/// Remembered on this iPad (not important if lost).
function load(key, fallback) {
  try {
    return JSON.parse(localStorage.getItem(key)) ?? fallback;
  } catch {
    return fallback;
  }
}
function store(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Storage full or blocked: only a convenience.
  }
}

/// A progress bar, `done` of `of`.
const meter = (done, of) => `<span class="meter"><span style="width:${Math.min(100, Math.round((done / of) * 100))}%"></span></span>`;

// ----- today's words ------------------------------------------------------------------
//
// The Mac sends its count of today's words with each sync. It includes what
// the iPad wrote up to the sync before (countedFrom); this iPad adds what it
// has saved since, from its own record of each save.

const WRITTEN = 'writer-written';

function localDay(d = new Date()) {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

function noteWritten(project, change, at) {
  if (!change) return;
  const day = localDay();
  const kept = load(WRITTEN, []).filter(w => w.date === day);
  kept.push({ project, words: change, at, date: day });
  store(WRITTEN, kept);
}

/// {words, goal} today for a project, from its index (as `api('index')` gives it).
function today(project, info) {
  const day = localDay();
  const mac = info.stats && info.stats.date === day ? info.stats.today || 0 : 0;
  const from = info.countedFrom || 0;
  const here = load(WRITTEN, []).filter(w => w.project === project && w.date === day && w.at > from)
    .reduce((sum, w) => sum + w.words, 0);
  return { words: Math.max(0, mac + here), goal: info.stats?.goal || null };
}

function todayHtml({ words: done, goal }) {
  return goal
    ? `<span class="today">Today ${fmt(done)} / ${fmt(goal)}${meter(done, goal)}</span>`
    : `<span class="today">Today ${plural(done, 'word', 'words')}</span>`;
}

const LAST = 'writer-last';      // the scene last opened, by project
const CLOSED = 'writer-closed';  // folded chapters, by project

function render(html) {
  app.innerHTML = html;
  // Which page is showing (for the tests, which wait for it).
  app.dataset.shown = location.hash;
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
/// Measuring means shrinking it for a moment; the page keeps its height
/// meanwhile, or Safari would scroll up to fit the shorter page (the text
/// jumping about while typing).
function autoGrow(textarea) {
  const fit = () => {
    const y = window.scrollY;
    app.style.minHeight = `${app.offsetHeight}px`;
    textarea.style.height = 'auto';
    textarea.style.height = `${textarea.scrollHeight + 4}px`;
    app.style.minHeight = '';
    if (window.scrollY !== y) window.scrollTo(0, y);
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
  const [info, inbox] = await Promise.all([api('index', undefined, q(project)), api('inbox', undefined, q(project))]);
  const { title, index } = info;
  const local = drafts();
  const hasDraft = s => Boolean(local[`${project}/${s.id}`]);
  const wordsOf = s => (hasDraft(s) ? words(local[`${project}/${s.id}`].text) : s.words || 0);
  // Grouped by the folder each is in, in the binder's order.
  const groups = [];
  for (const scene of index) {
    const last = groups[groups.length - 1];
    if (last && last.chapter === scene.chapter) last.scenes.push(scene);
    else groups.push({ chapter: scene.chapter, scenes: [scene] });
  }
  const closed = new Set(load(CLOSED, {})[project] || []);
  const lastScene = index.find(s => s.id === load(LAST, {})[project]);
  const stats = info.stats || {};
  const href = s => `#/p/${encodeURIComponent(project)}/s/${encodeURIComponent(s.id)}`;

  const row = s => {
    const w = wordsOf(s);
    return `<li data-find="${esc(`${s.title} ${s.synopsis || ''} ${s.status || ''}`.toLowerCase())}"><a href="${href(s)}">
      <span class="row-top"><strong>${esc(s.title)}</strong>${s.status ? `<span class="pill">${esc(s.status)}</span>` : ''}</span>
      ${s.synopsis ? `<span class="synopsis">${esc(s.synopsis)}</span>` : ''}
      <span class="meta">${s.target ? `${fmt(w)} / ${fmt(s.target)} words${meter(w, s.target)}` : plural(w, 'word', 'words')}
        ${hasDraft(s) ? '<span class="unsaved">not saved yet</span>' : ''}</span></a></li>`;
  };

  render(`${nav({ href: '#/', label: 'Projects' })}
    <header class="project-head">
      <h1>${esc(title)}</h1>
      <p class="figures">
        ${stats.total != null ? `<span>${fmt(stats.total)}${stats.target ? ` of ${fmt(stats.target)}` : ''} words${stats.target ? meter(stats.total, stats.target) : ''}</span>` : ''}
        ${todayHtml(today(project, info))}
      </p>
    </header>
    <div class="project-tools">
      ${lastScene ? `<a class="continue" href="${href(lastScene)}">Continue <strong>${esc(lastScene.title)}</strong></a>` : '<span></span>'}
      <a class="button" href="#/p/${encodeURIComponent(project)}/new">New Scene</a>
    </div>
    ${index.length > 5 ? '<input id="find" type="search" placeholder="Find a scene" autocomplete="off" aria-label="Find a scene">' : ''}
    ${groups.map(g => {
      const total = g.scenes.reduce((sum, s) => sum + wordsOf(s), 0);
      return `<details class="chapter" data-chapter="${esc(g.chapter)}" ${closed.has(g.chapter) ? '' : 'open'}>
        <summary><span class="name">${esc(g.chapter || title)}</span>
          <span class="quiet">${plural(g.scenes.length, 'scene', 'scenes')} · ${fmt(total)} words</span></summary>
        <ol class="scenes">${g.scenes.map(row).join('')}</ol></details>`;
    }).join('')}
    <p id="none" class="quiet" ${index.length ? 'hidden' : ''}>${index.length ? 'No scenes match.' : 'No scenes are on the iPad for this project.'}</p>
    ${inbox.length ? `<p class="quiet">${plural(inbox.length, 'new scene is', 'new scenes are')}
      waiting for your Mac to take into the Manuscript.</p>` : ''}`);

  // Folding a chapter is remembered.
  app.querySelectorAll('details.chapter').forEach(d => d.addEventListener('toggle', () => {
    if (app.querySelector('#find')?.value) return;  // opened by finding, not by hand
    const all = load(CLOSED, {});
    const set = new Set(all[project] || []);
    if (d.open) set.delete(d.dataset.chapter);
    else set.add(d.dataset.chapter);
    all[project] = [...set];
    store(CLOSED, all);
  }));
  // Finding: titles, synopses and statuses.
  const find = app.querySelector('#find');
  find?.addEventListener('input', () => {
    const wanted = find.value.trim().toLowerCase();
    let shown = 0;
    app.querySelectorAll('details.chapter').forEach(d => {
      let here = 0;
      d.querySelectorAll('li').forEach(li => {
        const match = !wanted || li.dataset.find.includes(wanted);
        li.hidden = !match;
        here += match;
      });
      d.hidden = here === 0;
      if (wanted && here) d.open = true;
      shown += here;
    });
    app.querySelector('#none').hidden = shown > 0;
  });
}

async function showScene(project, sceneId) {
  const key = `${project}/${sceneId}`;
  let info = { index: [] };
  let scene;
  try {
    info = await api('index', undefined, q(project));
    scene = await api('scene', undefined, q(project, sceneId));
  } catch (err) {
    if (!offline(err)) {
      if (err.status === 404) {
        const known = info.index.find(s => s.id === sceneId);
        render(`${nav({ href: `#/p/${encodeURIComponent(project)}`, label: 'Back' })}<h1>${esc(known?.title || 'Scene')}</h1>
          <p class="quiet">This scene hasn't reached the helper yet: Sloth Writer sends it at its next sync.</p>`);
        return;
      }
      throw err;
    }
  }
  const draft = drafts()[key];
  if (!scene && !draft) throw new TypeError('offline');
  const at = info.index.findIndex(s => s.id === sceneId);
  const entry = info.index[at] || {};
  const [prev, next] = [info.index[at - 1], info.index[at + 1]];
  store(LAST, { ...load(LAST, {}), [project]: sceneId });

  // Where to start: the helper's text, unless this iPad has unsaved words.
  let base = scene ? scene.version : draft.base;
  let saved = scene ? scene.text : draft.text;
  let text = draft && (!scene || draft.base === scene.version || draft.text !== scene.text) ? draft.text : saved;
  const title = entry.title || draft?.title || 'Scene';
  // Today's words when the page opened; what's typed here is added as it goes.
  const todayAtOpen = today(project, info);
  const sceneLink = s => `#/p/${encodeURIComponent(project)}/s/${encodeURIComponent(s.id)}`;

  render(`${nav({ href: `#/p/${encodeURIComponent(project)}`, label: entry.chapter || 'Back' })}
    <header class="scene-head">
      <h1>${esc(title)}</h1>
      <div class="tools">
        <span id="state" class="quiet"></span>
        <span id="count"></span>
        <span id="today"></span>
        <button id="toggle">Preview</button>
      </div>
    </header>
    <div id="clash" class="clash" hidden></div>
    <details id="notes-box" class="notes">
      <summary>Notes <span id="notes-glimpse" class="quiet"></span></summary>
      <textarea id="notes" readonly placeholder="Notes about this scene: not part of the book. They sync with Sloth Writer's notes."
        aria-label="Notes"></textarea>
    </details>
    <textarea id="text" spellcheck="true" autocapitalize="sentences" aria-label="${esc(title)}"></textarea>
    <article id="preview" class="preview" hidden></article>
    <nav class="scene-nav">
      ${prev ? `<a href="${sceneLink(prev)}" class="prev">‹ ${esc(prev.title)}</a>` : '<span></span>'}
      ${next ? `<a href="${sceneLink(next)}" class="next">${esc(next.title)} ›</a>` : '<span></span>'}
    </nav>`);
  const textarea = document.getElementById('text');
  const stateEl = document.getElementById('state');
  const countEl = document.getElementById('count');
  const todayEl = document.getElementById('today');
  textarea.value = text;
  emDashes(textarea);
  autoGrow(textarea);

  const show = state => (stateEl.textContent = state);
  // Words saved here since the page opened, for today's figure.
  let savedHere = 0;
  const count = () => {
    const n = words(textarea.value);
    countEl.innerHTML = entry.target ? `${fmt(n)} / ${fmt(entry.target)} words${meter(n, entry.target)}` : plural(n, 'word', 'words');
    const typed = n - words(saved);
    todayEl.innerHTML = todayHtml({ ...todayAtOpen, words: Math.max(0, todayAtOpen.words + savedHere + typed) });
  };
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
      const change = words(sending) - words(saved);
      noteWritten(project, change, r.savedAt);
      savedHere += change;
      base = r.version;
      saved = sending;
      count();
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

  const notes = sceneNotes(project, sceneId, title);

  // Back online, or every so often: send what's waiting.
  const retry = () => {
    if (textarea.value !== saved && !clash) save();
    notes.save();
  };
  window.addEventListener('online', retry);
  const every = setInterval(retry, 20000);
  // Leaving the page or the app: save what's typed (keepalive lets it
  // finish even as Safari puts the page away).
  const flush = () => {
    if (document.visibilityState !== 'hidden') return;
    save({ keepalive: true });
    notes.save({ keepalive: true });
  };
  document.addEventListener('visibilitychange', flush);
  leaving = async () => {
    await Promise.all([save(), notes.save()]);
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

/// A scene's notes, in the folding panel above its text. They're kept on
/// the helper as the scene "<id>-notes" and sync with the notes in Sloth
/// Writer. If the Mac changed them meanwhile, both are kept: the Mac's,
/// then these.
function sceneNotes(project, sceneId, title) {
  const id = `${sceneId}-notes`;
  const key = `${project}/${id}`;
  const box = document.getElementById('notes-box');
  const field = document.getElementById('notes');
  const glimpse = document.getElementById('notes-glimpse');
  let base = 0;
  let saved = '';
  let ready = false;
  let timer = null;
  let saving = false;

  const showGlimpse = () => {
    const first = field.value.split('\n').find(l => l.trim()) || '';
    glimpse.textContent = box.open || !first ? '' : `— ${first}`;
  };
  box.addEventListener('toggle', () => {
    showGlimpse();
    if (box.open) requestAnimationFrame(() => autoGrowOnce(field));
  });

  (async () => {
    let fromHelper = null;
    try {
      fromHelper = await api('scene', undefined, q(project, id));
    } catch (err) {
      if (err.status !== 404 && !offline(err)) throw err;
    }
    base = fromHelper ? fromHelper.version : 0;
    saved = fromHelper ? fromHelper.text : '';
    const draft = drafts()[key];
    field.value = draft ? draft.text : saved;
    field.readOnly = false;
    ready = true;
    if (field.value.trim()) box.open = true;
    showGlimpse();
    autoGrow(field);
    if (draft && draft.text !== saved) schedule();
  })().catch(() => {});

  async function save({ keepalive = false } = {}) {
    clearTimeout(timer);
    if (!ready || saving || field.value === saved) return;
    saving = true;
    const sending = field.value;
    try {
      const r = await api('scene_put', { text: sending, base }, q(project, id), { keepalive });
      base = r.version;
      saved = sending;
      if (field.value === saved) setDraft(key, null);
      else schedule();
    } catch (err) {
      if (err.status === 409) {
        // Changed on the Mac too: keep both.
        const theirs = err.data.current ? err.data.current.text.trim() : '';
        const mine = field.value.trim();
        field.value = !theirs || mine.includes(theirs) ? field.value : `${theirs}\n\n${field.value}`;
        base = err.data.current ? err.data.current.version : base;
        saving = false;
        return save();
      }
      // Offline: kept as a draft, sent later.
    } finally {
      saving = false;
    }
  }

  function schedule() {
    clearTimeout(timer);
    timer = setTimeout(save, 1500);
  }

  field.addEventListener('input', () => {
    if (!ready) return;
    setDraft(key, { text: field.value, base, title: `${title} (notes)` });
    schedule();
  });
  return { save };
}

/// Size a text area to its text now (for one that was hidden).
function autoGrowOnce(textarea) {
  textarea.style.height = 'auto';
  textarea.style.height = `${textarea.scrollHeight + 4}px`;
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
  const waiting = Object.entries(drafts()).filter(([key]) => !key.endsWith('/new') && !key.endsWith('-notes'));
  render(`<h1>Sloth Writer</h1><p class="quiet">Offline. Scenes you were writing here can still be opened;
    they're saved when you're back online.</p>
    <ul class="list">${waiting.map(([key, d]) => {
      const [project, scene] = key.split('/');
      return `<li><a href="#/p/${encodeURIComponent(project)}/s/${encodeURIComponent(scene)}"><strong>${esc(d.title)}</strong></a></li>`;
    }).join('')}</ul>`);
}

// ----- signing in, and the account -----------------------------------------------------

// The kinds of field, as password managers need to see them.
const USERNAME = { type: 'text', autocomplete: 'username' };
const CURRENT_PASSWORD = { type: 'password', autocomplete: 'current-password' };
const NEW_PASSWORD = { type: 'password', autocomplete: 'new-password', minlength: 8 };
// The setup password isn't the account's: a plain field, dotted by CSS, so
// password managers neither save it nor fill it in.
const SETUP_PASSWORD = { type: 'text', autocomplete: 'off', secret: true };

function fieldHtml({ name, label, type, autocomplete, minlength, secret }) {
  const attrs = [
    `id="${name}"`, `name="${name}"`, `type="${type}"`, `autocomplete="${autocomplete}"`, 'required',
    'autocapitalize="off"', 'spellcheck="false"',
    minlength ? `minlength="${minlength}"` : '',
    // Asks 1Password, LastPass and Bitwarden to leave it alone too.
    secret ? 'class="secret" data-1p-ignore data-lpignore="true" data-bwignore' : '',
  ];
  return `<label for="${name}">${label}</label><input ${attrs.filter(Boolean).join(' ')}>`;
}

/// A form of `fields` ({name, label, ...one of the kinds above}); `after`
/// goes below the button.
function form(title, fields, button, onSubmit, { intro = '', after = '' } = {}) {
  render(`<h1>${title}</h1>
    ${intro}
    <form>
      ${fields.map(fieldHtml).join('')}
      <p class="error" id="error"></p>
      <button class="primary">${button}</button>
    </form>
    ${after}`);
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
  form('Sloth Writer', [
    { name: 'username', label: 'Choose a username', ...USERNAME },
    { name: 'password', label: 'Choose a password (8 or more characters)', ...NEW_PASSWORD },
    { name: 'setupPassword', label: 'Setup password (from writer-config.php)', ...SETUP_PASSWORD },
  ], 'Create My Account', async v => { await api('setup', v); location.hash = '#/account'; route(); });
}

function showSignIn() {
  form('Sloth Writer', [
    { name: 'username', label: 'Username', ...USERNAME },
    { name: 'password', label: 'Password', ...CURRENT_PASSWORD },
  ], 'Sign In', async v => {
    await api('login', v);
    route();
  }, { after: '<p><a href="#" id="forgot">Forgot your password?</a></p>' });
  app.querySelector('#forgot').addEventListener('click', e => {
    e.preventDefault();
    showRecover();
  });
}

/// Locked out: the setup password from writer-config.php sets a new password.
function showRecover() {
  form('Forgot Password', [
    { name: 'username', label: 'Username', ...USERNAME },
    { name: 'setupPassword', label: 'Setup password (from writer-config.php)', ...SETUP_PASSWORD },
    { name: 'password', label: 'New password (8 or more characters)', ...NEW_PASSWORD },
  ], 'Set New Password', async v => {
    await api('recover', v);
    location.hash = '#/';
    route();
  }, {
    intro: `<p class="quiet">Enter the setup password from <code>writer-config.php</code> on your web host to choose a new password.
      Sloth Writer on your Mac keeps syncing: its token doesn't change.</p>`,
    after: '<p><a href="#" id="back">Back to Sign In</a></p>',
  });
  app.querySelector('#back').addEventListener('click', e => {
    e.preventDefault();
    showSignIn();
  });
}

async function showAccount(newToken, message = '') {
  const [tokens, me] = await Promise.all([api('tokens'), api('me')]);
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
    <h2>Password</h2>
    <form id="password-form">
      <input class="visually-hidden" type="text" name="username" autocomplete="username" value="${esc(me.user.username)}" tabindex="-1" aria-hidden="true">
      ${fieldHtml({ name: 'current', label: 'Current password', ...CURRENT_PASSWORD })}
      ${fieldHtml({ name: 'password', label: 'New password (8 or more characters)', ...NEW_PASSWORD })}
      <p class="error" id="password-error"></p>
      ${message ? `<p class="saved">${esc(message)}</p>` : ''}
      <button class="primary">Change Password</button>
    </form>
    <p class="quiet">Signed in as ${esc(me.user.username)}.</p>
    <p><button id="signout">Sign Out</button></p>`);
  app.querySelector('#password-form').addEventListener('submit', async e => {
    e.preventDefault();
    try {
      await api('change_password', { current: e.target.current.value, password: e.target.password.value });
      showAccount(undefined, 'Password changed.');
    } catch (err) {
      app.querySelector('#password-error').textContent = err.message;
    }
  });
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
