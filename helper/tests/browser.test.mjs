// The iPad page in headless Chrome, against the real helper: signing in,
// writing with autosave, em dashes, the preview, clashes with the Mac,
// offline drafts, and new scenes. Skipped without Chrome (see chrome.mjs).
// HELPER_SCREENSHOTS=folder saves pictures along the way.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { URL_BASE, ACCOUNT, mac, newProject } from './lib.mjs';
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

/// Go to a page of the app. Only the address's # changes, which loads
/// nothing, so this doesn't wait for a load the way goto does.
const go = hash => page.eval(`location.hash = ${JSON.stringify(hash)}`);

before(async () => {
  if (skip) return;
  // The Mac sends a project with two scenes in a chapter.
  theMac = await mac();
  project = newProject();
  await theMac.call('index_put', {
    title: 'The Long Winter',
    index: [
      { id: 'arrival', title: 'The arrival', chapter: 'Chapter 1', synopsis: 'Too late.', status: 'First draft', words: 4 },
      { id: 'letter', title: 'A letter', chapter: 'Chapter 1', synopsis: '', status: null, words: 2 },
    ],
  }, `&p=${project}`);
  await theMac.call('scene_put', { text: 'Mara was late.', base: 0 }, `&p=${project}&id=arrival`);
  await theMac.call('scene_put', { text: 'Dear Mara,', base: 0 }, `&p=${project}&id=letter`);

  page = await launch();
  // iPad-sized.
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
  await page.waitFor(`document.querySelectorAll('ul.list li').length === 2`, { what: 'the scene list' });
  const listed = await page.eval(`[...document.querySelectorAll('ul.list li strong')].map(s => s.textContent)`);
  assert.deepEqual(listed, ['The arrival', 'A letter']);
  assert.ok(await page.eval(`document.querySelector('h2').textContent === 'Chapter 1'`));
  assert.ok(await page.eval(`document.body.textContent.includes('First draft · 4 words')`));
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

  await typeAtEnd(' More from the iPad.');
  await page.waitFor(`!document.getElementById('clash').hidden`, { what: 'the clash message', timeout: 8000 });
  assert.equal(await state(), 'Changed on your Mac');
  await snap('clash');
  await page.eval(`document.getElementById('keep-mine').click()`);
  await page.waitFor(`document.getElementById('state').textContent === 'Saved'`, { timeout: 8000 });
  assert.match((await onHelper('arrival')).text, /More from the iPad\.$/);
});

test("a clash with the Mac: use the Mac's, and mine becomes a new scene", { skip }, async () => {
  await go(`#/p/${project}/s/letter`);
  await page.waitFor(`document.getElementById('text')?.value === 'Dear Mara,'`);
  await theMac.call('scene_put', { text: 'Dear Mara, from the Mac.', base: 1 }, `&p=${project}&id=letter`);
  await typeAtEnd(' from the iPad.');
  await page.waitFor(`!document.getElementById('clash').hidden`, { timeout: 8000 });
  await page.eval(`document.getElementById('use-macs').click()`);
  await page.waitFor(`document.getElementById('text').value === 'Dear Mara, from the Mac.'`);
  const inbox = (await theMac.call('inbox', undefined, `&p=${project}`)).body;
  assert.ok(inbox.some(i => i.title === 'A letter (iPad version)' && i.text === 'Dear Mara, from the iPad.'));
});

test('offline: kept on the iPad, sent when back', { skip }, async () => {
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
