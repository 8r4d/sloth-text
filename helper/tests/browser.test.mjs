// The web editor page in headless Chrome, against the real helper: signing in,
// writing with autosave, em dashes, the preview, clashes with the Mac,
// offline drafts, and new scenes. Skipped without Chrome (see chrome.mjs).
// HELPER_SCREENSHOTS=folder saves pictures along the way.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { execSync } from 'node:child_process';
import { URL_BASE, ACCOUNT, SETUP_PASSWORD, mac, newProject } from './lib.mjs';
import { chromePath, launch } from './chrome.mjs';

const skip = !chromePath && 'Chrome or Chromium not found (set CHROME to its path)';
// Which writing area to test: the paragraph editor, or (HELPER_EDITOR=plain) the text area.
const EDITOR = process.env.HELPER_EDITOR === 'plain' ? 'plain' : 'paragraphs';
const paragraphsOnly = skip || (EDITOR !== 'paragraphs' && 'for the paragraph editor');
let page, theMac, project;

const snap = async name => {
  if (process.env.HELPER_SCREENSHOTS) await page.screenshot(path.join(process.env.HELPER_SCREENSHOTS, `${name}.png`));
};

// The scene as the helper has it.
const onHelper = async id => (await theMac.call('scene', undefined, `&p=${project}&id=${id}`)).body;

/// Type at the end of the scene's text, as a keyboard would.
async function typeAtEnd(text) {
  await page.eval(`(() => { const t = document.getElementById('text'); t.focus(); t.setSelectionRange(t.value.length, t.value.length); })()`);
  for (const ch of text) await page.send('Input.insertText', { text: ch });
}

const state = () => page.eval(`document.getElementById('state').textContent`);

/// Today, as the page counts days.
function localDay(d = new Date()) {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/// Go to a page of the app. Only the address's # changes, which loads
/// nothing, so this doesn't wait for a load the way goto does.
/// It then waits until that page is showing.
async function go(hash) {
  await page.eval(`location.hash = ${JSON.stringify(hash)}`);
  await page.waitFor(`document.getElementById('app').dataset.shown === ${JSON.stringify(hash)}`, { what: `the page ${hash}` });
}

before(async () => {
  if (skip) return;
  // The Mac sends a project with two scenes in a chapter.
  theMac = await mac();
  project = newProject();
  await theMac.call('index_put', {
    title: 'The Long Winter',
    index: [
      { id: 'arrival', title: 'The arrival', chapter: 'Chapter 1', synopsis: 'Too late.', status: 'First draft', words: 4, target: 2000 },
      { id: 'letter', title: 'A letter', chapter: 'Chapter 1', synopsis: '', status: null, words: 2 },
    ],
    stats: { date: localDay(), today: 100, goal: 500, total: 6, target: 90000, deadline: null },
  }, `&p=${project}`);
  await theMac.call('scene_put', { text: 'Mara was late.', base: 0 }, `&p=${project}&id=arrival`);
  await theMac.call('scene_put', { text: 'Dear Mara,', base: 0 }, `&p=${project}&id=letter`);

  page = await launch();
  // Tablet-sized.
  await page.send('Emulation.setDeviceMetricsOverride', { width: 820, height: 1180, deviceScaleFactor: 1, mobile: true });
});
after(async () => { if (page) await page.close(); });

test('sign in, and the project and its scenes', { skip }, async () => {
  await page.goto(URL_BASE);
  await page.eval(`localStorage.setItem('writer-editor', ${JSON.stringify(JSON.stringify(EDITOR))})`);
  // Each script and stylesheet is addressed by its file's version (Cloudflare).
  const assets = await page.eval(`[...document.querySelectorAll('script[src], link[rel=stylesheet]')].map(e => e.getAttribute('src') || e.getAttribute('href'))`);
  assert.ok(assets.every(a => /\?v=\d{9,}$/.test(a)), JSON.stringify(assets));
  await page.waitFor(`document.getElementById('username')`, { what: 'the sign-in form' });
  await page.eval(`
    document.getElementById('username').value = ${JSON.stringify(ACCOUNT.username)};
    document.getElementById('password').value = ${JSON.stringify(ACCOUNT.password)};
    document.querySelector('form').requestSubmit();`);
  await page.waitFor(`document.body.textContent.includes('The Long Winter')`, { what: 'the project list' });
  await snap('projects');

  await go(`#/p/${project}`);
  await page.waitFor(`document.querySelectorAll('a.doc').length === 2`, { what: 'the scene list' });
  const listed = await page.eval(`[...document.querySelectorAll('a.doc strong')].map(s => s.textContent)`);
  assert.deepEqual(listed, ['The arrival', 'A letter']);
  assert.equal(await page.eval(`document.querySelector('details.folder .name').textContent`), 'Chapter 1');
  assert.equal(await page.eval(`document.querySelector('a.doc .pill').textContent`), 'First draft');
  const meta = await page.eval(`[...document.querySelectorAll('a.doc .meta')].map(m => m.textContent.trim())`);
  assert.deepEqual(meta, ['4 / 2,000 words', '2 words']);
  assert.match(await page.eval(`document.querySelector('.figures').textContent`), /Today 100 \/ 500/);
  await snap('scenes');
});

test('writing saves itself, against the version it started from', { skip }, async () => {
  await go(`#/p/${project}/s/arrival`);
  await page.waitFor(`document.getElementById('text')?.value === 'Mara was late.'`, { what: 'the scene' });
  await typeAtEnd(' The ferry had gone.');
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { what: 'the autosave', timeout: 8000 });
  assert.deepEqual(await onHelper('arrival'), { id: 'arrival', text: 'Mara was late. The ferry had gone.', version: 2 });
  assert.equal(await page.eval(`localStorage.getItem('writer-drafts')`), '{}', 'no draft left once saved');
});

test('two hyphens make an em dash', { skip }, async () => {
  await typeAtEnd(' Wait--what');
  assert.match(await page.eval(`document.getElementById('text').value`), / Wait—what$/);
  await typeAtEnd(' <!-- a -- note -->');
  assert.match(await page.eval(`document.getElementById('text').value`), /<!-- a -- note -->$/, 'not in notes');
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { timeout: 8000 });
});

test('the preview shows the formatting, without notes', { skip }, async () => {
  await typeAtEnd(' She was *very* late.');
  await page.eval(`document.getElementById('toggle').click()`);
  const html = await page.eval(`document.getElementById('preview').innerHTML`);
  assert.match(html, /<em>very<\/em>/);
  assert.doesNotMatch(html, /note/);
  await snap('preview');
  await page.eval(`document.getElementById('toggle').click()`);
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { timeout: 8000 });
});

test('a clash with the Mac: keep mine', { skip }, async () => {
  // The Mac saves the scene meanwhile.
  const { version } = await onHelper('arrival');
  await theMac.call('scene_put', { text: 'The Mac rewrote it.', base: version }, `&p=${project}&id=arrival`);

  await typeAtEnd(' More from the web.');
  await page.waitFor(`!document.getElementById('clash').hidden`, { what: 'the clash message', timeout: 8000 });
  assert.equal(await state(), 'Changed on your Mac');
  await snap('clash');
  await page.eval(`document.getElementById('keep-mine').click()`);
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { timeout: 8000 });
  assert.match((await onHelper('arrival')).text, /More from the web\.$/);
});

test("a clash with the Mac: use the Mac's, and mine becomes a new scene", { skip }, async () => {
  await go(`#/p/${project}/s/letter`);
  await page.waitFor(`document.getElementById('text')?.value === 'Dear Mara,'`);
  await theMac.call('scene_put', { text: 'Dear Mara, from the Mac.', base: 1 }, `&p=${project}&id=letter`);
  await typeAtEnd(' from the web.');
  await page.waitFor(`!document.getElementById('clash').hidden`, { timeout: 8000 });
  await page.eval(`document.getElementById('use-macs').click()`);
  await page.waitFor(`document.getElementById('text').value === 'Dear Mara, from the Mac.'`);
  const inbox = (await theMac.call('inbox', undefined, `&p=${project}`)).body;
  assert.ok(inbox.some(i => i.title === 'A letter (web version)' && i.text === 'Dear Mara, from the web.'));
});

test('offline: kept on the web, sent when back', { skip }, async () => {
  await go(`#/p/${project}/s/letter`);
  await page.waitFor(`document.getElementById('state')?.textContent === 'Saved'`);
  await page.setOffline(true);
  await typeAtEnd(' Written on a train.');
  await page.waitFor(`document.getElementById('state').textContent.startsWith('Offline')`, { what: 'the offline note', timeout: 8000 });
  const drafts = JSON.parse(await page.eval(`localStorage.getItem('writer-drafts')`));
  assert.match(drafts[`${project}/letter`].text, /Written on a train\.$/);
  await snap('offline');

  await page.setOffline(false);
  await page.eval(`window.dispatchEvent(new Event('online'))`);
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { timeout: 8000 });
  assert.match((await onHelper('letter')).text, /Written on a train\.$/);
});

test('a connection that hangs: kept here after a while, and the page moves on', { skip }, async () => {
  await go(`#/p/${project}/s/letter`);
  await page.waitFor(`document.getElementById('state')?.textContent === 'Saved'`);
  // Give up after 1.5 s (rather than 15), on a connection taking 6.
  await page.eval(`localStorage.setItem('writer-wait-ms', '1500')`);
  await page.setSlow(6000);
  await typeAtEnd(' Slowly.');
  const started = Date.now();
  await page.waitFor(`document.getElementById('state').textContent.startsWith('Offline')`, { what: 'the offline note', timeout: 6000 });
  assert.ok(Date.now() - started < 5000, 'not left saving');
  // Leaving isn't held up by the stuck save either.
  const left = Date.now();
  await go(`#/p/${project}`);
  assert.ok(Date.now() - left < 6000, `left in ${Date.now() - left} ms`);
  await page.setSlow(0);
  await page.eval(`localStorage.removeItem('writer-wait-ms')`);
  // Back with a connection, it's sent.
  await go(`#/p/${project}/s/letter`);
  await page.waitFor(`document.getElementById('state')?.textContent === 'Saved'`, { timeout: 8000 });
  assert.match((await onHelper('letter')).text, /Slowly\.$/);
});

test("a save that got there though its reply didn't: no false clash", { skip }, async () => {
  await go(`#/p/${project}/s/letter`);
  await page.waitFor(`document.getElementById('state')?.textContent === 'Saved'`);
  await page.setOffline(true);
  await typeAtEnd(' Reply lost.');
  await page.waitFor(`document.getElementById('state').textContent.startsWith('Offline')`, { timeout: 8000 });
  // The helper has these very words (the save reached it, the reply didn't).
  const typed = await page.eval(`document.getElementById('text').value`);
  const { version } = await onHelper('letter');
  await theMac.call('scene_put', { text: typed, base: version }, `&p=${project}&id=letter`);
  await page.setOffline(false);
  await page.eval(`window.dispatchEvent(new Event('online'))`);
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { timeout: 8000 });
  assert.equal(await page.eval(`document.getElementById('clash').hidden`), true, 'no clash');
  assert.equal(await page.eval(`localStorage.getItem('writer-drafts')`).then(d => (JSON.parse(d) || {})[`${project}/letter`]), undefined, 'nothing left waiting');
});

test('offline: projects opened here are kept, to read and write in', { skip }, async () => {
  const trip = newProject();
  await theMac.call('index_put', {
    title: 'Travel Pieces',
    index: [
      { id: 'harbour', title: 'The harbour', chapter: 'Essays', path: [{ id: 'manuscript', title: 'Manuscript' }], words: 3 },
      { id: 'trains', title: 'Trains', chapter: 'Essays', path: [{ id: 'manuscript', title: 'Manuscript' }], words: 2 },
    ],
    stats: { date: localDay(), today: 0, goal: null, total: 5, target: null, deadline: null },
  }, `&p=${trip}`);
  await theMac.call('scene_put', { text: 'The harbour froze.', base: 0 }, `&p=${trip}&id=harbour`);
  await theMac.call('scene_put', { text: 'Trains are late.', base: 0 }, `&p=${trip}&id=trains`);
  await theMac.call('scene_put', { text: 'Check the dates.', base: 0 }, `&p=${trip}&id=trains-notes`);
  await go('#/');
  // Opened with a connection: kept, documents and all, in one request.
  await go(`#/p/${trip}`);
  await page.waitFor(`JSON.parse(localStorage.getItem('writer-kept:${trip}') || '{}').whole === true`, { what: 'the project kept' });

  await page.setOffline(true);
  await go('#/');
  await page.waitFor(`document.querySelector('.offline-note')`, { what: 'the projects, as kept' });
  assert.match(await page.eval(`document.body.textContent`), /Travel Pieces/);
  await go(`#/p/${trip}`);
  await page.waitFor(`document.querySelectorAll('a.doc').length === 2`, { what: 'the documents, as kept' });
  await snap('offline-project');
  // One never opened here: there to read, and to write in, notes and all.
  await go(`#/p/${trip}/s/trains`);
  await page.waitFor(`document.getElementById('text')?.value === 'Trains are late.'`, { what: 'its text, as kept' });
  await page.waitFor(`document.getElementById('notes').value === 'Check the dates.'`, { what: 'its notes, as kept' });
  assert.ok(await page.eval(`Boolean(document.querySelector('.offline-note'))`));
  await typeAtEnd(' Very late.');
  await page.waitFor(`document.getElementById('state').textContent.startsWith('Offline')`, { timeout: 8000 });
  await snap('offline-writing');

  // The app opened from scratch with no connection (from the home screen,
  // which starts at the projects): the page from the service worker, the
  // projects and the project from this device.
  await page.goto(URL_BASE);
  await page.waitFor(`document.querySelector('.offline-note') && document.body.textContent.includes('Travel Pieces')`, { what: 'the projects, opened offline' });
  await go(`#/p/${trip}`);
  await page.waitFor(`document.querySelectorAll('a.doc').length === 2`, { what: 'the project, opened offline' });

  // Back online: what was written is sent, against the version it was kept at.
  await page.setOffline(false);
  await go(`#/p/${trip}/s/trains`);
  await page.waitFor(`document.getElementById('state')?.textContent === 'Saved'`, { timeout: 8000 });
  assert.equal((await theMac.call('scene', undefined, `&p=${trip}&id=trains`)).body.text, 'Trains are late. Very late.');
  assert.deepEqual(page.errors, [], 'no errors on the page');
});

test('a new scene goes to the inbox for the Mac', { skip }, async () => {
  await go(`#/p/${project}/new`);
  await page.waitFor(`document.getElementById('send')`);
  await page.eval(`document.getElementById('title').value = 'On the quay'; document.getElementById('title').dispatchEvent(new Event('input'));`);
  await typeAtEnd('A new idea.');
  await page.eval(`document.getElementById('send').click()`);
  await page.waitFor(`location.hash === '#/p/${project}'`, { what: 'back to the project' });
  const inbox = (await theMac.call('inbox', undefined, `&p=${project}`)).body;
  assert.ok(inbox.some(i => i.title === 'On the quay' && i.text === 'A new idea.'));
  // Sent with the page's own name for it, so a resend would make the same one.
  const keys = execSync(`${process.env.HELPER_DB} -N -e "SELECT client_key FROM writer_inbox WHERE title = 'On the quay' ORDER BY id DESC LIMIT 1"`).toString().trim();
  assert.match(keys, /^[0-9a-f-]{36}$/);
  await page.waitFor(`document.body.textContent.includes('waiting for your Mac')`);
  assert.deepEqual(page.errors, [], 'no errors on the page');
});

test("notes: in their panel, saved to the helper as the scene's notes", { skip }, async () => {
  await go(`#/p/${project}/s/arrival`);
  await page.waitFor(`document.getElementById('notes') && !document.getElementById('notes').readOnly`, { what: 'the notes, loaded' });
  assert.equal(await page.eval(`document.getElementById('notes-box').open`), false, 'folded while there are none');
  await page.eval(`document.getElementById('notes-box').open = true; document.getElementById('notes').focus()`);
  for (const ch of 'Check the ferry times.') await page.send('Input.insertText', { text: ch });
  await page.waitFor(`true`);
  const deadline = Date.now() + 8000;
  let notes;
  while (Date.now() < deadline) {
    notes = await onHelper('arrival-notes');
    if (notes.text === 'Check the ferry times.') break;
    await new Promise(r => setTimeout(r, 200));
  }
  assert.equal(notes.text, 'Check the ferry times.');
  // Opened again, they're shown straight away.
  await go(`#/p/${project}`);
  await go(`#/p/${project}/s/arrival`);
  await page.waitFor(`document.getElementById('notes')?.value === 'Check the ferry times.' && document.getElementById('notes-box').open`);
  await snap('notes');
});

test("today's words: the Mac's count, and what's written here since", { skip }, async () => {
  await go(`#/p/${project}`);
  await go(`#/p/${project}/s/letter`);
  await page.waitFor(`document.getElementById('today')?.textContent.startsWith('Today')`);
  const before = await page.eval(`document.getElementById('today').textContent`);
  const n = Number(before.match(/Today ([\d,]+)/)[1].replace(/,/g, ''));
  await typeAtEnd(' Three more words.');
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { timeout: 8000 });
  assert.match(await page.eval(`document.getElementById('today').textContent`), new RegExp(`Today ${n + 3} / 500`));
  // And on the scene list, after leaving.
  await go(`#/p/${project}`);
  await page.waitFor(`document.querySelector('.figures')`);
  assert.match(await page.eval(`document.querySelector('.figures').textContent`), new RegExp(`Today ${n + 3} / 500`));
});

test('the document list: continue where you were, fold a folder', { skip }, async () => {
  await page.waitFor(`document.querySelector('.continue')`);
  assert.match(await page.eval(`document.querySelector('.continue').textContent`), /Continue\s+A letter/);
  // Prev and next on the scene itself.
  await go(`#/p/${project}/s/arrival`);
  await page.waitFor(`document.querySelector('.scene-nav .next')`);
  assert.match(await page.eval(`document.querySelector('.scene-nav .next').textContent`), /A letter/);
  assert.equal(await page.eval(`document.querySelector('.scene-nav .prev')`), null);
  // Folding a chapter is remembered.
  await go(`#/p/${project}`);
  await page.waitFor(`document.querySelector('details.folder')`);
  await page.eval(`document.querySelector('details.folder > summary').click()`);
  await page.waitFor(`!document.querySelector('details.folder').open`);
  await go(`#/p/${project}/s/arrival`);
  await go(`#/p/${project}`);
  await page.waitFor(`document.querySelector('details.folder')`);
  assert.equal(await page.eval(`document.querySelector('details.folder').open`), false);
  await page.eval(`document.querySelector('details.folder > summary').click()`);
  await snap('scene-list');
});

test('nested folders: the binder as it is, finding with where things are, and the way back', { skip }, async () => {
  // A bigger project: a part holding chapters, a chapter broken up into
  // essays (a document with documents inside it), notes, and labels.
  const big = newProject();
  const m = { id: 'manuscript', title: 'Manuscript' };
  const part = { id: 'part1', title: 'Part One' };
  const ch1 = { id: 'ch1', title: 'Chapter 1' };
  const ch2 = { id: 'ch2', title: 'Chapter 2' };
  const notes = { id: 'notes', title: 'Notes' };
  await theMac.call('index_put', {
    title: 'Essays',
    index: [
      { id: 'intro', title: 'Introduction', chapter: 'Manuscript', path: [m], words: 50 },
      { id: 'walk', title: 'On Walking', chapter: 'Chapter 1', path: [m, part, ch1], words: 300, label: { name: 'Draft', color: '#e5484d' } },
      { id: 'rain', title: 'Rain', chapter: 'Chapter 1', path: [m, part, ch1], words: 200 },
      // Chapter 2 is a document: its own text, then the essays in it.
      { id: 'ch2', title: 'Chapter 2', chapter: 'Part One', path: [m, part], words: 40 },
      { id: 'trains', title: 'Trains', chapter: 'Chapter 2', path: [m, part, ch2], words: 400 },
      { id: 'ideas', title: 'Ideas', chapter: 'Notes', path: [notes], words: 10 },
      // Another "Rain", elsewhere: finding tells them apart.
      { id: 'rain2', title: 'Rain', chapter: 'Notes', path: [notes], words: 5 },
    ],
    stats: { date: localDay(), today: 0, goal: null, total: 990, target: null, deadline: null },
  }, `&p=${big}`);
  await theMac.call('scene_put', { text: 'It rained.', base: 0 }, `&p=${big}&id=rain`);

  await go(`#/p/${big}`);
  await page.waitFor(`document.querySelectorAll('a.doc').length === 7`, { what: 'the documents' });
  // Each folder where the binder has it, with what's in it (subfolders too).
  const tree = await page.eval(`(() => {
    const walk = el => [...el.children].map(c => c.matches('details.folder')
      ? { folder: c.querySelector(':scope > summary .name').textContent, count: c.querySelector(':scope > summary .quiet').textContent,
          inside: walk(c.querySelector(':scope > .inside')) }
      : c.querySelector('strong').textContent + (c.querySelector('.own') ? ' (own)' : ''));
    return walk(document.getElementById('tree'));
  })()`);
  assert.deepEqual(tree, [
    { folder: 'Manuscript', count: '5 documents · 990 words', inside: [
      'Introduction',
      { folder: 'Part One', count: '4 documents · 940 words', inside: [
        { folder: 'Chapter 1', count: '2 documents · 500 words', inside: ['On Walking', 'Rain'] },
        { folder: 'Chapter 2', count: '2 documents · 440 words', inside: ['Chapter 2 (own)', 'Trains'] },
      ] },
    ] },
    { folder: 'Notes', count: '2 documents · 15 words', inside: ['Ideas', 'Rain'] },
  ]);
  // A label, as a touch of its colour.
  assert.equal(await page.eval(`getComputedStyle([...document.querySelectorAll('a.doc')].find(a => a.textContent.includes('On Walking'))).borderLeftColor`), 'rgb(229, 72, 77)');
  await snap('nested');

  // Close All, then Open All; folding is kept by folder, not by name.
  await page.eval(`document.getElementById('close-all').click()`);
  assert.equal(await page.eval(`[...document.querySelectorAll('details.folder')].some(d => d.open)`), false);
  await page.eval(`document.getElementById('open-all').click()`);
  assert.equal(await page.eval(`[...document.querySelectorAll('details.folder')].every(d => d.open)`), true);
  await page.eval(`document.querySelector('details.folder[data-folder="ch1"] > summary').click()`);
  await go(`#/p/${big}/s/intro`);
  await go(`#/p/${big}`);
  await page.waitFor(`document.querySelector('details.folder[data-folder="ch1"]')`);
  assert.deepEqual(await page.eval(`[...document.querySelectorAll('details.folder')].filter(d => !d.open).map(d => d.dataset.folder)`), ['ch1']);

  // Finding: one list, each with where it is.
  await page.eval(`(() => { const f = document.getElementById('find'); f.value = 'rain'; f.dispatchEvent(new Event('input')); })()`);
  assert.equal(await page.eval(`document.getElementById('tree').hidden`), true);
  const found = await page.eval(`[...document.querySelectorAll('#results a.doc')].map(a => a.querySelector('strong').textContent + ' · ' + a.querySelector('.crumbs').textContent)`);
  // (Trains too: "rain" is in it.)
  assert.deepEqual(found, ['Rain · Manuscript \u203a Part One \u203a Chapter 1', 'Trains · Manuscript \u203a Part One \u203a Chapter 2', 'Rain · Notes']);
  await snap('nested-find');

  // Writing in one: where it is, above its title, and back to its folder.
  await go(`#/p/${big}/s/rain`);
  await page.waitFor(`document.querySelector('.scene-head .crumbs')`);
  assert.equal(await page.eval(`document.querySelector('.scene-head .crumbs').textContent.replace(/\\s+/g, ' ').trim()`),
    'Essays \u203a Manuscript \u203a Part One \u203a Chapter 1');
  assert.match(await page.eval(`document.querySelector('nav.bar .back').textContent`), /Chapter 1/);
  // Next leads into another folder: it says which.
  assert.match(await page.eval(`document.querySelector('.scene-nav .next').textContent`), /Chapter 2\s+in Part One/);
  await snap('nested-crumbs');

  // Back: the project at Chapter 1, opened (though it was folded) and in view.
  await page.eval(`document.querySelector('nav.bar .back').click()`);
  await page.waitFor(`document.getElementById('app').dataset.shown === '#/p/${big}/f/ch1'`, { what: 'the folder' });
  assert.equal(await page.eval(`document.querySelector('details.folder[data-folder="ch1"]').open`), true);
  assert.equal(await page.eval(`document.querySelector('details.folder[data-folder="ch1"]').classList.contains('focused')`), true);
  assert.deepEqual(page.errors, [], 'no errors on the page');
});

test('typing at the end of a long scene keeps the page where it is', { skip }, async () => {
  await go(`#/p/${project}/s/arrival`);
  // Settled: the scene and its notes loaded.
  await page.waitFor(`document.getElementById('state')?.textContent === 'Saved' && !document.getElementById('notes').readOnly`);
  // A long scene, scrolled to its end.
  await page.eval(`(() => {
    const t = document.getElementById('text');
    t.value += '\\n' + Array.from({ length: 150 }, (_, i) => 'Paragraph ' + i + ' of a long scene.').join('\\n');
    t.dispatchEvent(new Event('input'));
    t.focus();
    t.setSelectionRange(t.value.length, t.value.length);
    window.scrollTo(0, document.body.scrollHeight);
  })()`);
  await page.waitFor(`window.scrollY > 1000`, { what: 'scrolled to the end' });
  const start = await page.eval(`window.scrollY`);
  assert.ok(start > 1000, `scrolled down (${start})`);
  for (const ch of ' more') await page.send('Input.insertText', { text: ch });
  const after = await page.eval(`window.scrollY`);
  assert.ok(after >= start - 2, `stayed put: ${start} → ${after}`);
  await page.eval(`document.getElementById('text').value = document.getElementById('text').value.split('\\nParagraph 0')[0]; document.getElementById('text').dispatchEvent(new Event('input'))`);
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { timeout: 8000 });
  assert.deepEqual(page.errors, [], 'no errors on the page');
});

test('password managers see the account password, and Forgot Password works', { skip }, async () => {
  await page.send('Network.clearBrowserCookies');
  await page.goto(URL_BASE);
  await page.waitFor(`document.getElementById('username')`, { what: 'the sign-in form' });
  const kinds = () => page.eval(`[...document.querySelectorAll('input')].map(i => [i.name, i.type, i.autocomplete])`);
  assert.deepEqual(await kinds(), [['username', 'text', 'username'], ['password', 'password', 'current-password']]);

  await page.eval(`document.getElementById('forgot').click()`);
  await page.waitFor(`document.getElementById('setupPassword')`, { what: 'the forgot-password form' });
  // The setup password isn't a password field, so it's never saved as the account's.
  assert.deepEqual(await kinds(), [
    ['username', 'text', 'username'], ['setupPassword', 'text', 'off'], ['password', 'password', 'new-password'],
  ]);
  assert.equal(await page.eval(`getComputedStyle(document.getElementById('setupPassword')).webkitTextSecurity`), 'disc');
  await snap('forgot-password');
  await page.eval(`
    document.getElementById('username').value = ${JSON.stringify(ACCOUNT.username)};
    document.getElementById('setupPassword').value = ${JSON.stringify(SETUP_PASSWORD)};
    document.getElementById('password').value = ${JSON.stringify(ACCOUNT.password)};
    document.querySelector('form').requestSubmit();`);
  await page.waitFor(`document.body.textContent.includes('The Long Winter')`, { what: 'signed in, the projects' });

  // The account page offers a password change, with the username for password managers.
  await go('#/account');
  await page.waitFor(`document.getElementById('current')`, { what: 'the change-password form' });
  const fields = await page.eval(`[...document.querySelectorAll('#password-form input')].map(i => [i.name, i.autocomplete])`);
  assert.deepEqual(fields, [['username', 'username'], ['current', 'current-password'], ['password', 'new-password']]);
  assert.deepEqual(page.errors, [], 'no errors on the page');
});

test('the mobile notepad: on the project list, first in the project, started here with dated entries', { skip }, async () => {
  await go('#/');
  // This project's (another test's project may be listed too).
  const link = `document.querySelector('a.notepad-link[href^="#/p/${project}/"]')`;
  await page.waitFor(link, { what: 'the notepad link' });
  assert.equal(await page.eval(`${link}.getAttribute('href')`), `#/p/${project}/s/notepad`);
  await snap('notepad-projects');

  await go(`#/p/${project}`);
  assert.match(await page.eval(`document.querySelector('.notepad-row').textContent`), /Mobile Notes\s+Start a notepad/);
  assert.doesNotMatch(await page.eval(`document.querySelector('details.folder').textContent`), /Mobile Notes/);
  await snap('notepad-in-project');

  // Not on the helper yet: it starts empty, and is made when written in.
  await go(`#/p/${project}/s/notepad`);
  await page.waitFor(`document.getElementById('text')`);
  assert.equal(await page.eval(`document.querySelector('.scene-head h1').textContent`), 'Mobile Notes');
  assert.equal(await page.eval(`document.getElementById('notes-box')`), null, 'no scene notes');
  assert.equal(await page.eval(`document.querySelector('.scene-nav a')`), null, 'no previous or next scene');
  await page.eval(`document.getElementById('entry').click()`);
  assert.match(await page.eval(`document.getElementById('text').value`), /^## \w+ .+, .+\n$/, 'a dated heading (in this browser\'s way of writing dates)');
  await typeAtEnd('The ferry timetable is wrong.');
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { what: 'the autosave', timeout: 8000 });
  const saved = await onHelper('notepad');
  assert.match(saved.text, /^## .+\nThe ferry timetable is wrong\.$/);
  assert.equal(saved.version, 1);
  // Notes aren't the day's writing.
  assert.equal(await page.eval(`document.getElementById('today').textContent`), '');
  await snap('notepad');

  // A second entry goes below the first.
  await page.eval(`document.getElementById('entry').click()`);
  assert.match(await page.eval(`document.getElementById('text').value`), /wrong\.\n\n## .+\n$/);
});

test('the mobile notepad: added to on the Mac meanwhile, both are kept', { skip }, async () => {
  await typeAtEnd('Second thought.');
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { timeout: 8000 });
  const { text, version } = await onHelper('notepad');
  await theMac.call('scene_put', { text: `${text}\n\nFrom the Mac: check the tides.`, base: version }, `&p=${project}&id=notepad`);

  await typeAtEnd(' And the lighthouse.');
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { what: 'the merged save', timeout: 10000 });
  assert.equal(await page.eval(`document.getElementById('clash').hidden`), true, 'no clash to sort out');
  const merged = (await onHelper('notepad')).text;
  assert.match(merged, /Second thought\.\n\nFrom the Mac: check the tides\.\n\nAnd the lighthouse\.\n?$/);
  assert.equal(await page.eval(`document.getElementById('text').value`), merged);
});

// ----- the paragraph editor ---------------------------------------------------------

/// ⌘ and a letter, as a keyboard would.
async function command(letter) {
  const code = `Key${letter.toUpperCase()}`;
  const keyCode = letter.toUpperCase().charCodeAt(0);
  await page.send('Input.dispatchKeyEvent', { type: 'keyDown', key: letter, code, windowsVirtualKeyCode: keyCode, modifiers: 4 });
  await page.send('Input.dispatchKeyEvent', { type: 'keyUp', key: letter, code, windowsVirtualKeyCode: keyCode, modifiers: 4 });
}

/// Press a key, as a keyboard would.
async function press(key, code, keyCode, text) {
  await page.send('Input.dispatchKeyEvent', { type: 'keyDown', key, code, windowsVirtualKeyCode: keyCode, text });
  await page.send('Input.dispatchKeyEvent', { type: 'keyUp', key, code, windowsVirtualKeyCode: keyCode });
}

test('paragraphs: each line its own, spaced and indented as Sloth Writer has them', { skip: paragraphsOnly }, async () => {
  // The Mac sends its settings with the day's figures, and a scene of three paragraphs.
  await theMac.call('index_put', {
    title: 'The Long Winter',
    index: [
      { id: 'arrival', title: 'The arrival', chapter: 'Chapter 1', words: 4 },
      { id: 'letter', title: 'A letter', chapter: 'Chapter 1', words: 2 },
      { id: 'ice', title: 'Ice', chapter: 'Chapter 2', words: 9 },
    ],
    stats: { date: localDay(), today: 0, goal: null, total: 0, target: null, deadline: null, look: { spacing: 1, indent: 1.5 } },
  }, `&p=${project}`);
  await theMac.call('scene_put', { text: 'The harbour froze.\n\nNobody came.\nNot even Tom.', base: 0 }, `&p=${project}&id=ice`);

  await go(`#/p/${project}/s/ice`);
  await page.waitFor(`document.querySelectorAll('#text p').length === 4`, { what: 'four paragraphs (one empty)' });
  assert.equal(await page.eval(`document.getElementById('text').value`), 'The harbour froze.\n\nNobody came.\nNot even Tom.');
  const look = await page.eval(`(() => {
    const [first, empty] = document.querySelectorAll('#text p');
    const s = getComputedStyle(first), size = parseFloat(s.fontSize);
    return { gap: parseFloat(s.marginBottom) / size, indent: parseFloat(s.textIndent) / size, emptyGap: parseFloat(getComputedStyle(empty).marginBottom) };
  })()`);
  assert.ok(Math.abs(look.gap - 1.65) < 0.05, `a line's space after: ${JSON.stringify(look)}`);
  assert.ok(Math.abs(look.indent - 1.5) < 0.05, `indented 1.5 em: ${JSON.stringify(look)}`);
  assert.equal(look.emptyGap, 0, 'no extra space after an empty line');
  await snap('paragraphs');
});

test('paragraphs: Return starts one, typing keeps plain spaces, and it all saves as lines', { skip: paragraphsOnly }, async () => {
  await typeAtEnd(' He waited.');
  await press('Enter', 'Enter', 13, '\r');
  await typeAtEnd('Then  the  bell.');
  const value = await page.eval(`document.getElementById('text').value`);
  assert.equal(value, 'The harbour froze.\n\nNobody came.\nNot even Tom. He waited.\nThen  the  bell.');
  assert.equal(await page.eval(`document.querySelectorAll('#text p').length`), 5);
  assert.ok(!value.includes(' '), 'ordinary spaces, not non-breaking ones');
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { what: 'the autosave', timeout: 8000 });
  assert.equal((await onHelper('ice')).text, value);
});

test('paragraphs: pasting brings plain text, and and ⌘B writes Markdown', { skip: paragraphsOnly }, async () => {
  await page.eval(`(() => {
    const t = document.getElementById('text');
    t.focus();
    t.setSelectionRange(t.value.length, t.value.length);
    const data = new DataTransfer();
    data.setData('text/html', '<p><b>Bold</b> words</p><p>and more</p>');
    data.setData('text/plain', ' Bold words\\nand more');
    t.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
  })()`);
  assert.match(await page.eval(`document.getElementById('text').value`), /bell\. Bold words\nand more$/);
  assert.equal(await page.eval(`document.querySelector('#text b, #text strong, #text span, #text div')`), null, 'no formatting');
  assert.equal(await page.eval(`[...document.getElementById('text').children].every(c => c.tagName === 'P')`), true);

  // ⌘B writes Markdown (and the browser's own bold is never used).
  await page.eval(`(() => { const t = document.getElementById('text'); t.setSelectionRange(0, 3); })()`);
  await command('b');
  await page.waitFor(`document.getElementById('text').value.startsWith('**The**')`, { what: '**The**' });
  assert.equal(await page.eval(`document.querySelector('#text b')`), null);
  await command('b');
  await page.waitFor(`document.getElementById('text').value.startsWith('The harbour')`, { what: 'off again' });
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { timeout: 8000 });
});

test('paragraphs: em dashes, and switching back to the plain box under Account', { skip: paragraphsOnly }, async () => {
  await typeAtEnd(' Wait--what');
  assert.match(await page.eval(`document.getElementById('text').value`), / Wait—what$/);
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { timeout: 8000 });

  await go('#/account');
  await page.waitFor(`document.getElementById('paragraphs')`);
  assert.equal(await page.eval(`document.getElementById('paragraphs').checked`), true);
  await page.eval(`document.getElementById('paragraphs').click()`);
  await go(`#/p/${project}/s/ice`);
  await page.waitFor(`document.getElementById('text')?.tagName === 'TEXTAREA'`, { what: 'the plain box' });
  assert.match(await page.eval(`document.getElementById('text').value`), / Wait—what$/);
  await page.eval(`localStorage.setItem('writer-editor', JSON.stringify('paragraphs'))`);
});

// ----- colours ------------------------------------------------------------------------

test("colours: the page wears Sloth Writer's theme, or this device's own", { skip }, async () => {
  const background = () => page.eval(`getComputedStyle(document.body).backgroundColor`);
  // The Mac's theme (Nord, as Sloth Writer sends it) comes with the day's figures.
  const nord = { dark: true, bg: '#2e3440', text: '#d8dee9', quiet: '#9aa0ab', line: '#434c5e', accent: '#88c0d0',
    tint: 'rgba(136, 192, 208, 0.12)', selection: '#434c5e', danger: '#e8897d' };
  await theMac.call('index_put', {
    title: 'The Long Winter',
    index: [{ id: 'arrival', title: 'The arrival', chapter: 'Chapter 1', words: 4 }],
    stats: { date: localDay(), today: 0, goal: null, total: 0, target: null, deadline: null, look: { spacing: 0, indent: 0, theme: nord } },
  }, `&p=${project}`);
  await go(`#/p/${project}`);
  await page.waitFor(`getComputedStyle(document.body).backgroundColor === 'rgb(46, 52, 64)'`, { what: 'Nord' });
  assert.equal(await page.eval(`getComputedStyle(document.querySelector('h1')).color`), 'rgb(216, 222, 233)');
  assert.equal(await page.eval(`document.querySelector('meta[name="theme-color"]').content`), '#2e3440', 'the status bar too');
  assert.equal(await page.eval(`getComputedStyle(document.documentElement).colorScheme`), 'dark');
  await snap('colours-nord');

  // Remembered for next time, before anything's loaded.
  await page.goto(URL_BASE);
  assert.equal(await background(), 'rgb(46, 52, 64)');
  await go('#/account');
  await page.waitFor(`document.getElementById('match-colors')`);

  // This device's own colours instead (the tests' browser is in light mode)…
  await page.eval(`document.getElementById('match-colors').click()`);
  assert.equal(await background(), 'rgb(246, 241, 231)');
  assert.equal(await page.eval(`document.querySelector('meta[name="theme-color"]').content`), '#f6f1e7');
  // …and back.
  await page.eval(`document.getElementById('match-colors').click()`);
  assert.equal(await background(), 'rgb(46, 52, 64)');

  // Match System on the Mac sends none: the device's own again.
  await theMac.call('index_put', {
    title: 'The Long Winter',
    index: [{ id: 'arrival', title: 'The arrival', chapter: 'Chapter 1', words: 4 }],
    stats: { date: localDay(), today: 0, goal: null, total: 0, target: null, deadline: null, look: { spacing: 0, indent: 0, theme: null } },
  }, `&p=${project}`);
  await go(`#/p/${project}`);
  await page.waitFor(`getComputedStyle(document.body).backgroundColor === 'rgb(246, 241, 231)'`, { what: 'the device colours' });
});

test("the Severance themes bring their office look, and the old screen's glow", { skip }, async () => {
  // Severance, as Sloth Writer sends it, with Old screen glow on.
  const severance = { dark: true, bg: '#061219', text: '#b4ebf2', quiet: '#7fb0b9', line: '#16343f', accent: '#3fd0e4',
    tint: 'rgba(63, 208, 228, 0.12)', selection: '#11414d', danger: '#e8897d' };
  const put = look => theMac.call('index_put', {
    title: 'The Long Winter',
    index: [{ id: 'arrival', title: 'The arrival', chapter: 'Chapter 1', words: 4 }],
    stats: { date: localDay(), today: 0, goal: null, total: 0, target: null, deadline: null, look: { spacing: 0, indent: 0, ...look } },
  }, `&p=${project}`);
  await put({ theme: severance, office: true, glow: true });
  await go('#/');
  await go(`#/p/${project}`);
  await page.waitFor(`document.documentElement.hasAttribute('data-glow')`, { what: 'the glow' });
  assert.ok(await page.eval(`document.documentElement.hasAttribute('data-office')`));
  assert.equal(await page.eval(`getComputedStyle(document.querySelector('h1')).color`), 'rgb(63, 208, 228)', 'headings in the accent');
  assert.match(await page.eval(`getComputedStyle(document.body, '::after').backgroundImage`), /repeating-linear-gradient/, 'scanlines');
  assert.equal(await page.eval(`getComputedStyle(document.body, '::after').pointerEvents`), 'none', 'never in the way');
  await snap('severance-glow');

  // Glow off there: the office look stays, the scanlines go.
  await put({ theme: severance, office: true, glow: false });
  await go('#/');
  await go(`#/p/${project}`);
  await page.waitFor(`!document.documentElement.hasAttribute('data-glow')`, { what: 'no glow' });
  assert.ok(await page.eval(`document.documentElement.hasAttribute('data-office')`));

  // Another theme: neither.
  await put({ theme: null });
  await go('#/');
  await go(`#/p/${project}`);
  await page.waitFor(`!document.documentElement.hasAttribute('data-office')`, { what: 'no office look' });
});

// ----- Markdown on the web: shortcuts, styling, the goal ring ------------------------------

const valueNow = () => page.eval(`document.getElementById('text').value`);

/// Give the scene "arrival" known words, and open it.
async function openArrival(text) {
  const { version } = await onHelper('arrival');
  await theMac.call('scene_put', { text, base: version }, `&p=${project}&id=arrival`);
  // Earlier tests leave words of theirs on this device.
  await page.eval(`localStorage.removeItem('writer-drafts')`);
  await go(`#/p/${project}`);
  await go(`#/p/${project}/s/arrival`);
  await page.waitFor(`document.getElementById('text')?.value === ${JSON.stringify(text)}`, { what: 'the scene' });
}

test('the paragraph editor is the one a device gets unless it chose the plain box', { skip }, async () => {
  await page.eval(`localStorage.removeItem('writer-editor')`);
  await openArrival('The harbour froze.');
  assert.equal(await page.eval(`document.getElementById('text').tagName`), 'DIV');
  await page.eval(`localStorage.setItem('writer-editor', ${JSON.stringify(JSON.stringify(EDITOR))})`);
});

test('⌘B and ⌘I write Markdown, and take it off again', { skip }, async () => {
  await openArrival('The harbour froze.');
  await page.eval(`(() => { const t = document.getElementById('text'); t.focus(); t.setSelectionRange(4, 11); })()`);
  await command('b');
  await page.waitFor(`document.getElementById('text').value === 'The **harbour** froze.'`, { what: 'bold' });
  await command('b');
  await page.waitFor(`document.getElementById('text').value === 'The harbour froze.'`, { what: 'off again' });
  await command('i');
  await page.waitFor(`document.getElementById('text').value === 'The *harbour* froze.'`, { what: 'italic' });
  await command('i');
  await page.waitFor(`document.getElementById('text').value === 'The harbour froze.'`, { what: 'off again' });
  // An iPad keyboard's I button.
  if (EDITOR === 'paragraphs') {
    await page.eval(`document.getElementById('text').dispatchEvent(new InputEvent('beforeinput', { inputType: 'formatItalic', bubbles: true, cancelable: true }))`);
    await page.waitFor(`document.getElementById('text').value === 'The *harbour* froze.'`, { what: 'the I button' });
  }
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { timeout: 8000 });
});

test('paragraphs show their Markdown once the cursor has left them', { skip: paragraphsOnly }, async () => {
  // A heading and some emphasis at the end; then the cursor goes elsewhere.
  await page.eval(`(() => { const t = document.getElementById('text'); t.focus(); t.setSelectionRange(t.value.length, t.value.length); })()`);
  const before = await valueNow();
  await press('Enter', 'Enter', 13, '\r');
  await typeAtEnd('## Spring');
  await press('Enter', 'Enter', 13, '\r');
  await typeAtEnd('The ice went out **all at once**, and *quietly*.');
  await page.eval(`document.getElementById('text').setSelectionRange(0, 0)`);
  await page.waitFor(`document.querySelector('#text strong')`, { what: 'bold, styled' });
  const shown = await page.eval(`(() => {
    const ps = [...document.querySelectorAll('#text p')];
    return { heading: ps.at(-2).className, strong: ps.at(-1).querySelector('strong').textContent, em: ps.at(-1).querySelector('em').textContent,
             marks: [...ps.at(-1).querySelectorAll('.mk')].map(m => m.textContent) };
  })()`);
  assert.deepEqual(shown, { heading: 'h', strong: 'all at once', em: 'quietly', marks: ['**', '**', '*', '*'] });
  assert.equal(await valueNow(), `${before}\n## Spring\nThe ice went out **all at once**, and *quietly*.`, 'the text itself unchanged');
  await snap('styled');

  // Typing on after a closing mark doesn't come out faint.
  await typeAtEnd(' Then it was gone.');
  const faint = await page.eval(`[...document.querySelectorAll('#text .mk')].some(m => m.textContent.includes('gone'))`);
  assert.equal(faint, false);
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { timeout: 8000 });
});

test("the day's goal as a ring, and the streak", { skip }, async () => {
  await theMac.call('index_put', {
    title: 'The Long Winter',
    index: [{ id: 'ice', title: 'Ice', chapter: 'Chapter 2', words: 9 }],
    stats: { date: localDay(), today: 120, goal: 500, total: 9, target: null, deadline: null, streak: 5 },
  }, `&p=${project}`);
  await go(`#/`);
  await go(`#/p/${project}`);
  await page.waitFor(`document.querySelector('.figures .ring')`, { what: 'the ring' });
  assert.match(await page.eval(`document.querySelector('.figures').textContent`), /Today \d+ \/ 500\s*5 days in a row/);
  await snap('ring');
});
