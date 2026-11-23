// Sloth Writer on the web: the scenes marked "Available on the web" in Sloth
// Writer, to read and write in a browser. Talks to api.php; the Mac syncs with
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
// say), the words are kept on this device as a draft.
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
// Comments ({==passage==}{>>comment<<}, from Sloth Writer) aren't words of
// the scene, nor notes; a commented passage is.
const withoutComments = text => text.replace(/\{>>[\s\S]*?<<\}/g, '').replace(/\{==([\s\S]*?)==\}/g, '$1');
const words = text => (withoutComments(text).replace(/<!--[\s\S]*?(-->|$)/g, ' ').match(/\S+/g) || []).length;
const fmt = n => Number(n || 0).toLocaleString();
const plural = (n, one, many) => `${fmt(n)} ${n === 1 ? one : many}`;

/// Remembered on this device (not important if lost).
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
// the web editor wrote up to the sync before (countedFrom); this device adds what it
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

/// A ring filling towards a goal (`done` of `of`), as in Sloth Writer's
/// status bar; a dot in the middle once it's met.
function ring(done, of) {
  const r = 6, round = 2 * Math.PI * r;
  const part = Math.min(1, of ? done / of : 0);
  // (No spaces between the tags: they'd count as the figure's text.)
  return '<svg class="ring" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">'
    + `<circle cx="8" cy="8" r="${r}" class="ring-track"/>`
    + `<circle cx="8" cy="8" r="${r}" class="ring-fill" stroke-dasharray="${(part * round).toFixed(2)} ${round.toFixed(2)}"/>`
    + (part >= 1 ? '<circle cx="8" cy="8" r="2" class="ring-done"/>' : '')
    + '</svg>';
}

function todayHtml({ words: done, goal }) {
  return goal
    ? `<span class="today">${ring(done, goal)}Today ${fmt(done)} / ${fmt(goal)}</span>`
    : `<span class="today">Today ${plural(done, 'word', 'words')}</span>`;
}

/// "12 days in a row", from the Mac's last sync (nothing under two).
function streakHtml(info) {
  const days = info.stats?.streak || 0;
  return days >= 2 ? `<span class="streak">${fmt(days)} days in a row</span>` : '';
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
/// so their --> still closes them). A device's own smart punctuation often does
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
/// meanwhile, or the browser would scroll up to fit the shorter page (the text
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
  return withoutComments(text)
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


// ----- the paragraph editor -----------------------------------------------------------
//
// A writing area with each line its own paragraph, so it can be spaced and
// indented as Sloth Writer shows it (a <textarea> can only show one block of
// evenly spaced lines). It stays plain text: what's typed, pasted or dropped
// is read back as lines, and any formatting the browser adds is dropped.
//
// To the rest of the page it looks like a <textarea>: `value`, `selectionStart`
// and `selectionEnd`, `setSelectionRange` and `setRangeText`, and `input`
// events, so the scene page (drafts, clashes, em dashes, the notepad) works the
// same with either. It's switched on per device, under Account.

const EDITOR = 'writer-editor';  // 'paragraphs' or 'plain', on this device
const LOOK = 'writer-look';      // Sloth Writer's paragraph spacing and indent, as last synced

// The paragraph editor, unless this device chose the plain box.
const usesParagraphs = () => load(EDITOR, 'paragraphs') === 'paragraphs';

// ----- colours ---------------------------------------------------------------------
//
// Sloth Writer sends its theme's colours with each sync (look.theme; none
// for Match System). The page wears them, unless this device is set (under
// Account) to its own light or dark mode, the stylesheet's colours.

const COLORS = 'writer-colors';  // 'writer' (Sloth Writer's theme) or 'device'
const THEME_VARS = { bg: '--bg', text: '--text', quiet: '--quiet', line: '--line', accent: '--accent', tint: '--tint', danger: '--danger', selection: '--selection' };

function applyColors() {
  const theme = load(COLORS, 'writer') === 'writer' ? load(LOOK, {}).theme : null;
  const root = document.documentElement;
  for (const [key, name] of Object.entries(THEME_VARS)) {
    if (theme?.[key]) root.style.setProperty(name, theme[key]);
    else root.style.removeProperty(name);
  }
  root.toggleAttribute('data-themed', Boolean(theme));
  // Form controls and scroll bars to match, and the phone's status bar.
  root.style.colorScheme = theme ? (theme.dark ? 'dark' : 'light') : '';
  document.querySelectorAll('meta[name="theme-color"]').forEach(m => {
    if (!m.dataset.device) m.dataset.device = m.content;
    m.content = theme ? theme.bg : m.dataset.device;
  });
}

/// Keep the look (spacing, indent, colours) from a project's index.
function keepLook(info) {
  if (!info.stats?.look) return;
  store(LOOK, info.stats.look);
  applyColors();
}

/// The writing area's HTML: a paragraph editor or a plain text area.
function writingArea({ label, placeholder = '' }) {
  const attrs = `id="text" spellcheck="true" autocapitalize="sentences" aria-label="${esc(label)}"`;
  return usesParagraphs()
    ? `<div ${attrs} class="writing" data-placeholder="${esc(placeholder)}"></div>`
    : `<textarea ${attrs} placeholder="${esc(placeholder)}"></textarea>`;
}

/// Wrap the selection in Markdown's emphasis (`**` bold, `*` italic), or
/// take it off if it's there already. (A text area's own setRangeText
/// doesn't say it changed the text, so this does.)
function emphasis(el, mark) {
  const value = el.value;
  const [start, end] = [el.selectionStart, el.selectionEnd];
  const n = mark.length;
  const wrapped = value.slice(start - n, start) === mark && value.slice(end, end + n) === mark
    // `*` inside `**`: that's bold, not italic.
    && !(n === 1 && value[start - 2] === '*' && value[end + 1] === '*');
  if (wrapped) {
    el.setRangeText(value.slice(start, end), start - n, end + n);
    el.setSelectionRange(start - n, end - n);
  } else {
    el.setRangeText(`${mark}${value.slice(start, end)}${mark}`, start, end);
    el.setSelectionRange(start + n, end + n);
  }
  if (el.tagName === 'TEXTAREA') el.dispatchEvent(new Event('input'));
}

/// ⌘B and ⌘I (and an iPad keyboard's B and I) write Markdown's ** and *.
/// The paragraph editor hears them as the browser's own "format bold"
/// edit (as the iPad's B and I buttons are); a text area, as the keys.
function markdownKeys(el) {
  el.addEventListener('keydown', e => {
    if (el.tagName !== 'TEXTAREA' || !(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey) return;
    const mark = { b: '**', i: '*' }[e.key.toLowerCase()];
    if (!mark) return;
    e.preventDefault();
    emphasis(el, mark);
  });
  el.addEventListener('beforeinput', e => {
    const mark = { formatBold: '**', formatItalic: '*' }[e.inputType];
    // After the browser's done with this edit (which it's told not to make).
    if (mark) setTimeout(() => emphasis(el, mark));
  });
}

/// Ready the writing area made by `writingArea`, holding `text`.
function setUpWriting(el, text) {
  if (el.tagName === 'TEXTAREA') {
    el.value = text;
    emDashes(el);
    autoGrow(el);
    markdownKeys(el);
    return el;
  }
  paragraphEditor(el);
  el.value = text;
  const look = load(LOOK, {});
  el.style.setProperty('--paragraph-space', String(Math.max(0, Number(look.spacing) || 0)));
  el.style.setProperty('--first-indent', `${Math.max(0, Number(look.indent) || 0)}em`);
  emDashes(el);
  markdownKeys(el);
  return el;
}

const BLOCKS = /^(P|DIV|H[1-6]|LI|UL|OL|BLOCKQUOTE|PRE|SECTION|ARTICLE)$/;

/// The lines of text in an element, as the paragraph editor reads them: each
/// block a line, each <br> a line break. A <br> that ends a block is only
/// there to keep an empty line open, so it doesn't count.
function readLines(root) {
  const lines = [''];
  let fresh = true;  // at the start of a line no text has gone into yet
  const breakLine = () => { lines.push(''); fresh = true; };
  const walk = node => {
    for (const child of node.childNodes) {
      if (child.nodeType === Node.TEXT_NODE) {
        lines[lines.length - 1] += child.data;
        fresh = false;
      } else if (child.nodeName === 'BR') {
        const last = child === node.lastChild && BLOCKS.test(node.nodeName) && node !== root;
        if (!last) breakLine();
      } else if (child.nodeType === Node.ELEMENT_NODE && BLOCKS.test(child.nodeName)) {
        if (!fresh || lines.length > 1 || lines[0] !== '') breakLine();
        walk(child);
        fresh = false;
      } else if (child.nodeType === Node.ELEMENT_NODE) {
        walk(child);
      }
    }
  };
  walk(root);
  // The first block doesn't start a new line.
  if (root.firstChild && BLOCKS.test(root.firstChild.nodeName) && lines[0] === '' && lines.length > 1) lines.shift();
  return lines;
}

/// The elements the editor's own styling uses inside a paragraph.
const STYLING = /^(SPAN|STRONG|EM)$/;

/// Is the editor in its tidy shape: only <p>s, each holding text (perhaps
/// in the editor's styling), and at most a closing <br>?
function tidy(root) {
  if (!root.firstChild) return false;
  const inline = node => [...node.childNodes].every(k =>
    (k.nodeType === Node.TEXT_NODE && !k.data.includes('\n')) || (STYLING.test(k.nodeName) && inline(k)));
  for (const p of root.childNodes) {
    if (p.nodeName !== 'P') return false;
    const kids = [...p.childNodes];
    for (let i = 0; i < kids.length; i++) {
      const k = kids[i];
      if (k.nodeType === Node.TEXT_NODE && !k.data.includes('\n')) continue;
      if (STYLING.test(k.nodeName) && inline(k)) continue;
      if (k.nodeName === 'BR' && i === kids.length - 1) continue;
      return false;
    }
  }
  return true;
}

// ----- styling as you type -----------------------------------------------------------
//
// As in Sloth Writer, the text shows what its Markdown does (bold, italics,
// headings, quotes, scene breaks), with the marks kept, faintly; notes,
// references and comments are quieter. A paragraph is restyled when the
// cursor's left it, never while it's being typed in (so nothing moves under
// the cursor, or under autocorrect and dictation).

const EMPHASIS = /(\*\*\*|\*\*|\*)(\S(?:.*?\S)?)\1/g;
const ASIDES = /(<!--[\s\S]*?-->|\[\[[^\]\n]*\]\]|\{>>[\s\S]*?<<\}|\{==|==\})/g;

/// Bold and italics in a stretch of a line, as HTML (the marks kept).
function emphasisHtml(text) {
  let out = '', from = 0;
  for (const m of text.matchAll(EMPHASIS)) {
    const [whole, mark, inner] = m;
    const [open, close] = mark.length === 3 ? ['<strong><em>', '</em></strong>'] : mark.length === 2 ? ['<strong>', '</strong>'] : ['<em>', '</em>'];
    out += esc(text.slice(from, m.index)) + `<span class="mk">${mark}</span>${open}${esc(inner)}${close}<span class="mk">${mark}</span>`;
    from = m.index + whole.length;
  }
  return out + esc(text.slice(from));
}

/// A line as the editor shows it: its kind (for the paragraph's class) and
/// its HTML, every character of it still there.
function styleLine(line) {
  const trimmed = line.trim();
  if (/^(#|(\*\s*){3,}|(-\s*){3,})$/.test(trimmed)) return { kind: 'brk', html: `<span class="mk">${esc(line)}</span>` };
  let kind = '', lead = '', rest = line;
  const heading = line.match(/^(\s*#{1,6}\s+)(.*)$/);
  const quote = line.match(/^(\s*>\s?)(.*)$/);
  if (heading) [kind, lead, rest] = ['h', heading[1], heading[2]];
  else if (quote) [kind, lead, rest] = ['q', quote[1], quote[2]];
  let html = lead ? `<span class="mk">${esc(lead)}</span>` : '';
  let from = 0;
  for (const m of rest.matchAll(ASIDES)) {
    html += emphasisHtml(rest.slice(from, m.index)) + `<span class="aside">${esc(m[0])}</span>`;
    from = m.index + m[0].length;
  }
  return { kind, html: html + emphasisHtml(rest.slice(from)) };
}

/// Show a paragraph styled, if its text has changed since it last was.
function styleParagraph(p) {
  const text = p.textContent;
  if (p.dataset.styled === text) return;
  p.dataset.styled = text;
  if (!text) {
    p.replaceChildren(document.createElement('br'));
    p.className = '';
    return;
  }
  const { kind, html } = styleLine(text);
  p.innerHTML = html;
  p.className = kind;
}

function paragraphEditor(el) {
  el.contentEditable = 'true';
  el.setAttribute('role', 'textbox');
  el.setAttribute('aria-multiline', 'true');
  try {
    document.execCommand('defaultParagraphSeparator', false, 'p');
  } catch {
    // Older browsers: the tidying below copes with what they make instead.
  }

  const text = () => readLines(el).join('\n');
  const build = value => {
    el.replaceChildren(...value.split('\n').map(line => {
      const p = document.createElement('p');
      p.textContent = line;
      styleParagraph(p);
      return p;
    }));
    el.toggleAttribute('data-empty', value === '');
  };

  /// Where a point in the editor is, as an offset into `value`.
  const offsetOf = (node, offset) => {
    const range = document.createRange();
    range.setStart(el, 0);
    range.setEnd(node, offset);
    const box = document.createElement('div');
    box.append(range.cloneContents());
    return readLines(box).join('\n').length;
  };
  /// The point in the (tidy) editor at an offset into `value`.
  const pointAt = offset => {
    let left = Math.max(0, offset);
    const paragraphs = [...el.children];
    for (const [i, p] of paragraphs.entries()) {
      const length = p.textContent.length;
      if (left <= length || i === paragraphs.length - 1) {
        let target = Math.min(left, length);
        // Into its text, if it has any (it may be split into several
        // nodes, and into the styling's).
        const walker = document.createTreeWalker(p, NodeFilter.SHOW_TEXT);
        for (let t = walker.nextNode(); t; t = walker.nextNode()) {
          if (target <= t.data.length) return [t, target];
          target -= t.data.length;
        }
        return [p, 0];
      }
      left -= length + 1;
    }
    return [el, 0];
  };
  const selection = () => {
    const sel = window.getSelection();
    if (!sel.rangeCount || !el.contains(sel.anchorNode)) return null;
    return sel.getRangeAt(0);
  };
  let remembered = [0, 0];
  const range = () => {
    const r = selection();
    if (r) remembered = [offsetOf(r.startContainer, r.startOffset), offsetOf(r.endContainer, r.endOffset)];
    return remembered;
  };
  const select = (start, end = start) => {
    remembered = [start, end];
    const sel = window.getSelection();
    const r = document.createRange();
    r.setStart(...pointAt(start));
    r.setEnd(...pointAt(end));
    sel.removeAllRanges();
    sel.addRange(r);
  };

  Object.defineProperties(el, {
    value: { get: text, set: value => build(String(value ?? '')) },
    selectionStart: { get: () => range()[0] },
    selectionEnd: { get: () => range()[1] },
  });
  el.setSelectionRange = (start, end) => select(start, end);
  // As a text area's, but through the browser's own editing, so Undo has it.
  el.setRangeText = (replacement, start, end) => {
    select(start, end);
    document.execCommand('insertText', false, replacement);
  };

  // Plain text only: no bold from ⌘B, and pasting brings the words, not their look.
  el.addEventListener('beforeinput', e => {
    if (e.inputType.startsWith('format')) e.preventDefault();
  });
  el.addEventListener('paste', e => {
    e.preventDefault();
    const pasted = (e.clipboardData?.getData('text/plain') || '').replace(/\r\n?/g, '\n');
    if (pasted) document.execCommand('insertText', false, pasted);
  });
  // After each change, back into tidy paragraphs if the browser made
  // something else (a <div> for a new line, a pasted <span>…), keeping the
  // cursor where it was. (Registered first, so the page sees the tidy text.)
  el.addEventListener('input', () => {
    if (!tidy(el)) {
      const r = selection();
      const at = r ? offsetOf(r.endContainer, r.endOffset) : null;
      build(text());
      if (at !== null) select(at);
    }
    el.toggleAttribute('data-empty', el.textContent === '' && el.children.length <= 1);
    // Typed on the end of a mark (after a closing **, say), the words went
    // into it, and would show faint: this paragraph is restyled at once,
    // the cursor kept where it is.
    const r = selection();
    const here = r && [...el.children].find(p => p.contains(r.endContainer));
    const strayed = mk => !/^(\*{1,3}|\s*#{1,6}\s+|\s*>\s?|[#*\-\s]+)$/.test(mk.textContent);
    if (here && !composing && [...here.querySelectorAll('.mk')].some(strayed)) {
      const at = range()[1];
      delete here.dataset.styled;
      styleParagraph(here);
      select(at);
    }
    restyle();
  });

  // Every paragraph but the cursor's shows its styling, brought up to date
  // when the cursor leaves it (and not mid-word for dictation or an input
  // method: they compose in place).
  let composing = false;
  el.addEventListener('compositionstart', () => { composing = true; });
  el.addEventListener('compositionend', () => { composing = false; restyle(); });
  const restyle = () => {
    if (composing) return;
    const r = selection();
    const here = r && [...el.children].find(p => p.contains(r.startContainer) || p.contains(r.endContainer));
    for (const p of el.children) {
      if (p !== here) styleParagraph(p);
    }
  };
  document.addEventListener('selectionchange', () => {
    if (el.isConnected) restyle();
  });
  return el;
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
    ${projects.length ? '' : `<p class="quiet">Nothing here yet. In Sloth Writer on your Mac, tick <em>Available on the web</em>
      in a chapter's inspector, and put this page's address and a token (from <a href="#/account">Account</a>) in its Settings.</p>`}
    <ul class="list projects">
      ${projects.map(p => `<li><a class="project" href="#/p/${encodeURIComponent(p.id)}"><strong>${esc(p.title)}</strong>
        <span class="quiet">${p.scenes} ${p.scenes === 1 ? 'scene' : 'scenes'}</span></a>
        <a class="notepad-link" href="${notepadHref(p.id)}" title="Mobile Notes"
          aria-label="Mobile Notes for ${esc(p.title)}">${PEN}</a></li>`).join('')}
    </ul>`);
}

// ----- the mobile notepad ------------------------------------------------------
// One document per project, for jotting when out and about. On the helper it's
// always the scene "notepad", whichever document it is on the Mac; it can be
// started here before the Mac has one (the Mac makes one at its next sync).

const NOTEPAD = 'notepad';
// A pen, drawn in the text's colour (an emoji's look varies from phone to phone).
const PEN = `<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" fill="none" stroke="currentColor"
  stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20l4.5-1 10-10a2.1 2.1 0 0 0-3-3l-10 10L4 20z"/>
  <path d="M14 7.5l2.5 2.5"/></svg>`;
const notepadHref = project => `#/p/${encodeURIComponent(project)}/s/${NOTEPAD}`;

// The notepad is added to from both sides, so when both changed since `base`,
// both sets of additions are kept, the Mac's first. (As Sloth Writer does:
// see merge_additions in sync.rs.) With no shared start, the two are joined.
function mergePad(base, macs, mine) {
  const m = macs.trimEnd(), t = mine.trimEnd();
  if (!t.trim() || m.includes(t.trim())) return macs;
  if (!m.trim() || t.includes(m.trim())) return mine;
  let start = (base || '').trimEnd();
  if (!m.startsWith(start) || !t.startsWith(start)) {
    // The lines they have in common at the start.
    let i = 0;
    while (i < m.length && m[i] === t[i]) i++;
    const cut = m.slice(0, i).lastIndexOf('\n');
    start = cut > 0 ? m.slice(0, cut).trimEnd() : '';
  }
  const added = s => s.slice(start.length).trim();
  return `${start}${start ? '\n\n' : ''}${added(m)}\n\n${added(t)}\n`;
}

// "## Mon 23 Nov, 2:05 pm": a new entry's heading.
function entryHeading(at = new Date()) {
  const day = at.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' });
  const time = at.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
  return `## ${day.replace(/,/g, '')}, ${time}`;
}

async function showProject(project) {
  const [info, inbox] = await Promise.all([api('index', undefined, q(project)), api('inbox', undefined, q(project))]);
  keepLook(info);
  const { title } = info;
  const notepad = info.index.find(s => s.id === NOTEPAD);
  const index = info.index.filter(s => s.id !== NOTEPAD);
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
        ${streakHtml(info)}
      </p>
    </header>
    <a class="notepad-row" href="${notepadHref(project)}"><span>📝 <strong>${esc(notepad?.title || 'Mobile Notes')}</strong></span>
      <span class="quiet">${notepad ? plural(wordsOf(notepad), 'word', 'words') : 'Start a notepad'}</span></a>
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
    <p id="none" class="quiet" ${index.length ? 'hidden' : ''}>${index.length ? 'No scenes match.' : 'No scenes are on the web for this project.'}</p>
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
  const isPad = sceneId === NOTEPAD;
  let info = { index: [] };
  let scene;
  try {
    info = await api('index', undefined, q(project));
    keepLook(info);
    scene = await api('scene', undefined, q(project, sceneId));
  } catch (err) {
    if (!offline(err)) {
      // A notepad not started yet: it is when something's written.
      if (err.status === 404 && isPad) {
        scene = { text: '', version: 0 };
      } else if (err.status === 404) {
        const known = info.index.find(s => s.id === sceneId);
        render(`${nav({ href: `#/p/${encodeURIComponent(project)}`, label: 'Back' })}<h1>${esc(known?.title || 'Scene')}</h1>
          <p class="quiet">This scene hasn't reached the helper yet: Sloth Writer sends it at its next sync.</p>`);
        return;
      } else {
        throw err;
      }
    }
  }
  const draft = drafts()[key];
  if (!scene && !draft) throw new TypeError('offline');
  const entry = info.index.find(s => s.id === sceneId) || {};
  // The scenes before and after, in the binder's order (the notepad isn't one).
  const scenes = info.index.filter(s => s.id !== NOTEPAD);
  const at = scenes.findIndex(s => s.id === sceneId);
  const [prev, next] = at < 0 ? [] : [scenes[at - 1], scenes[at + 1]];
  if (!isPad) store(LAST, { ...load(LAST, {}), [project]: sceneId });

  // Where to start: the helper's text, unless this device has unsaved words.
  let base = scene ? scene.version : draft.base;
  let saved = scene ? scene.text : draft.text;
  let text = draft && (!scene || draft.base === scene.version || draft.text !== scene.text) ? draft.text : saved;
  // The notepad: words kept here from before the Mac's last change join its.
  if (isPad && scene && draft && draft.base !== scene.version) {
    text = mergePad(null, scene.text, draft.text);
  }
  const title = entry.title || draft?.title || (isPad ? 'Mobile Notes' : 'Scene');
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
        ${isPad ? '<button id="entry" class="entry">New Entry</button>' : ''}
        <button id="toggle">Preview</button>
      </div>
    </header>
    <div id="clash" class="clash" hidden></div>
    ${isPad ? '' : `<details id="notes-box" class="notes">
      <summary>Notes <span id="notes-glimpse" class="quiet"></span></summary>
      <textarea id="notes" readonly placeholder="Notes about this scene: not part of the book. They sync with Sloth Writer's notes."
        aria-label="Notes"></textarea>
    </details>`}
    ${writingArea({ label: title })}
    <article id="preview" class="preview" hidden></article>
    <nav class="scene-nav">
      ${prev ? `<a href="${sceneLink(prev)}" class="prev">‹ ${esc(prev.title)}</a>` : '<span></span>'}
      ${next ? `<a href="${sceneLink(next)}" class="next">${esc(next.title)} ›</a>` : '<span></span>'}
    </nav>`);
  const textarea = setUpWriting(document.getElementById('text'), text);
  const stateEl = document.getElementById('state');
  const countEl = document.getElementById('count');
  const todayEl = document.getElementById('today');

  const show = state => (stateEl.textContent = state);
  // Words saved here since the page opened, for today's figure.
  let savedHere = 0;
  const count = () => {
    const n = words(textarea.value);
    countEl.innerHTML = entry.target ? `${fmt(n)} / ${fmt(entry.target)} words${meter(n, entry.target)}` : plural(n, 'word', 'words');
    if (isPad) return;  // notes, not the day's writing
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
      const change = isPad ? 0 : words(sending) - words(saved);
      if (!isPad) noteWritten(project, change, r.savedAt);
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
      if (err.status === 409 && isPad && err.data.current) {
        // Added to on the Mac meanwhile: keep both, and send that.
        const { text: macs, version } = err.data.current;
        const merged = mergePad(saved, macs, textarea.value);
        base = version;
        saved = macs;
        textarea.value = merged;
        textarea.dispatchEvent(new Event('input'));
      } else if (err.status === 409) {
        showClash(err.data.current);
      } else if (offline(err)) {
        show('Offline: kept on this device');
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

  // The Mac changed the scene while this device was writing it.
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
        await api('inbox_add', { title: `${title} (web version)`, text: mine }, q(project));
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

  const notes = isPad ? { save: async () => {} } : sceneNotes(project, sceneId, title);

  if (isPad) {
    // A new entry: the date and time as a heading, and the cursor under it.
    document.getElementById('entry').onclick = () => {
      const before = textarea.value.trimEnd();
      textarea.value = `${before}${before ? '\n\n' : ''}${entryHeading()}\n`;
      textarea.dispatchEvent(new Event('input'));
      textarea.focus();
      textarea.setSelectionRange(textarea.value.length, textarea.value.length);
      window.scrollTo(0, document.body.scrollHeight);
    };
    // Opened at the end, where the newest notes are.
    window.scrollTo(0, document.body.scrollHeight);
  }

  // Back online, or every so often: send what's waiting.
  const retry = () => {
    if (textarea.value !== saved && !clash) save();
    notes.save();
  };
  window.addEventListener('online', retry);
  const every = setInterval(retry, 20000);
  // Leaving the page or the app: save what's typed (keepalive lets it
  // finish even as the browser puts the page away).
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
    if (!isPad && scene && draft && draft.base !== scene.version) showClash({ text: scene.text, version: scene.version });
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
    ${writingArea({ label: 'New scene', placeholder: 'Write…' })}
    <p class="error" id="error"></p>
    <button id="send" class="primary">Send to My Mac</button>`);
  const textarea = setUpWriting(document.getElementById('text'), draft.text);
  const titleEl = document.getElementById('title');
  const keep = () => setDraft(key, { title: titleEl.value, text: textarea.value });
  textarea.addEventListener('input', keep);
  titleEl.addEventListener('input', keep);
  document.getElementById('send').onclick = async () => {
    try {
      await api('inbox_add', { title: titleEl.value, text: textarea.value }, q(project));
      setDraft(key, null);
      location.hash = `#/p/${encodeURIComponent(project)}`;
    } catch (err) {
      document.getElementById('error').textContent = offline(err) ? 'Offline: kept on this device; send it when you\'re back online.' : err.message;
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
    <h2>Writing on this device</h2>
    <label class="check"><input type="checkbox" id="paragraphs" ${usesParagraphs() ? 'checked' : ''}>
      Paragraphs as in Sloth Writer</label>
    <p class="quiet">Each paragraph spaced and indented as Sloth Writer's Settings have them. Turn it off for the plain
      writing box.</p>
    <label class="check"><input type="checkbox" id="match-colors" ${load(COLORS, 'writer') === 'writer' ? 'checked' : ''}>
      Colours from Sloth Writer's theme</label>
    <p class="quiet">Off: this device's own light or dark mode.</p>
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
  app.querySelector('#paragraphs').addEventListener('change', e => store(EDITOR, e.target.checked ? 'paragraphs' : 'plain'));
  app.querySelector('#match-colors').addEventListener('change', e => {
    store(COLORS, e.target.checked ? 'writer' : 'device');
    applyColors();
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

// The last colours, at once, before anything's drawn in the wrong ones.
applyColors();
window.addEventListener('hashchange', route);
// index.php names the worker with its files' versions (see sw.js).
if ('serviceWorker' in navigator) navigator.serviceWorker.register(window.WORKER || 'sw.js').catch(() => {});
start();
