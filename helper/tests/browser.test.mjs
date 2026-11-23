// The web editor page in headless Chrome, against the real helper: signing in,
// writing with autosave, em dashes, the preview, clashes with the Mac,
// offline drafts, and new scenes. Skipped without Chrome (see chrome.mjs).
// HELPER_SCREENSHOTS=folder saves pictures along the way.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { URL_BASE, ACCOUNT, SETUP_PASSWORD, mac, newProject } from './lib.mjs';
import { chromePath, launch } from './chrome.mjs';

const skip = !chromePath && 'Chrome or Chromium not found (set CHROME to its path)';
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
  await page.waitFor(`document.querySelectorAll('ol.scenes li').length === 2`, { what: 'the scene list' });
  const listed = await page.eval(`[...document.querySelectorAll('ol.scenes li strong')].map(s => s.textContent)`);
  assert.deepEqual(listed, ['The arrival', 'A letter']);
  assert.equal(await page.eval(`document.querySelector('details.chapter .name').textContent`), 'Chapter 1');
  assert.equal(await page.eval(`document.querySelector('ol.scenes .pill').textContent`), 'First draft');
  const meta = await page.eval(`[...document.querySelectorAll('ol.scenes .meta')].map(m => m.textContent.trim())`);
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

test('a new scene goes to the inbox for the Mac', { skip }, async () => {
  await go(`#/p/${project}/new`);
  await page.waitFor(`document.getElementById('send')`);
  await page.eval(`document.getElementById('title').value = 'On the quay'; document.getElementById('title').dispatchEvent(new Event('input'));`);
  await typeAtEnd('A new idea.');
  await page.eval(`document.getElementById('send').click()`);
  await page.waitFor(`location.hash === '#/p/${project}'`, { what: 'back to the project' });
  const inbox = (await theMac.call('inbox', undefined, `&p=${project}`)).body;
  assert.ok(inbox.some(i => i.title === 'On the quay' && i.text === 'A new idea.'));
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

test('the scene list: continue where you were, find a scene, fold a chapter', { skip }, async () => {
  await page.waitFor(`document.querySelector('.continue')`);
  assert.match(await page.eval(`document.querySelector('.continue').textContent`), /Continue\s+A letter/);
  // Prev and next on the scene itself.
  await go(`#/p/${project}/s/arrival`);
  await page.waitFor(`document.querySelector('.scene-nav .next')`);
  assert.match(await page.eval(`document.querySelector('.scene-nav .next').textContent`), /A letter/);
  assert.equal(await page.eval(`document.querySelector('.scene-nav .prev')`), null);
  // Folding a chapter is remembered.
  await go(`#/p/${project}`);
  await page.waitFor(`document.querySelector('details.chapter')`);
  await page.eval(`document.querySelector('details.chapter > summary').click()`);
  await page.waitFor(`!document.querySelector('details.chapter').open`);
  await go(`#/p/${project}/s/arrival`);
  await go(`#/p/${project}`);
  await page.waitFor(`document.querySelector('details.chapter')`);
  assert.equal(await page.eval(`document.querySelector('details.chapter').open`), false);
  await page.eval(`document.querySelector('details.chapter > summary').click()`);
  await snap('scene-list');
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
