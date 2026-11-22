// api.php against a real (throwaway) database. Run by tests/run.sh.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execSync } from 'node:child_process';
import { Client, ACCOUNT, SETUP_PASSWORD, browser, mac, newProject } from './lib.mjs';

// Forgets every password try, so one test's lock doesn't spill into the next.
const forgetTries = () => execSync(`${process.env.HELPER_DB} -e "DELETE FROM writer_login_attempts"`);

test('setup makes the one account, once', async () => {
  await browser();
  const again = await new Client().call('setup', { setupPassword: SETUP_PASSWORD, username: 'second', password: 'whatever-123' });
  assert.equal(again.status, 403);
});

test('signing in: right and wrong passwords, and too many tries', async () => {
  forgetTries();
  const c = new Client();
  assert.equal((await c.call('login', { ...ACCOUNT, password: 'wrong-password' })).status, 403);
  assert.equal((await c.call('login', ACCOUNT)).status, 200);
  assert.equal((await c.call('me')).body.user.username, ACCOUNT.username);

  forgetTries();
  const guesser = new Client();
  for (let i = 0; i < 5; i++) await guesser.call('login', { ...ACCOUNT, password: `guess-${i}` });
  const blocked = await guesser.call('login', ACCOUNT);
  assert.equal(blocked.status, 429, 'even the right password waits after five wrong ones');
  forgetTries();
});

test('nothing without signing in or a token', async () => {
  const stranger = new Client();
  assert.equal((await stranger.call('projects')).status, 401);
  assert.equal((await new Client('not-a-real-token').call('projects')).status, 401);
});

test('tokens: made in a browser, shown once, listed, used by the Mac, revoked', async () => {
  const b = await browser();
  const made = await b.call('token_create', { label: 'Studio iMac' });
  assert.equal(made.status, 200);
  assert.match(made.body.token, /^[0-9a-f]{64}$/);

  const theMac = new Client(made.body.token);
  assert.equal((await theMac.call('projects')).status, 200);
  const listed = (await b.call('tokens')).body.find(t => t.id === made.body.id);
  assert.equal(listed.label, 'Studio iMac');
  assert.ok(listed.lastUsedAt, 'last used is recorded');
  assert.equal(listed.token, undefined, 'the token itself is never shown again');

  // A token can't make more tokens.
  assert.equal((await theMac.call('token_create', { label: 'x' })).status, 403);

  await b.call('token_delete', {}, `&id=${made.body.id}`);
  assert.equal((await theMac.call('projects')).status, 401);
});

test('the index: the scenes marked for the iPad', async () => {
  const m = await mac();
  const p = newProject();
  assert.equal((await m.call('index', undefined, `&p=${p}`)).status, 404);
  const index = [{ id: 'abc123', title: 'The arrival', chapter: 'Chapter 1', synopsis: 'Too late.' }];
  assert.equal((await m.call('index_put', { title: 'The Long Winter', index }, `&p=${p}`)).status, 200);

  // The web editor sees it.
  const b = await browser();
  const got = await b.call('index', undefined, `&p=${p}`);
  assert.deepEqual(got.body, { title: 'The Long Winter', index, seq: 0 });
  assert.ok((await b.call('projects')).body.some(x => x.id === p && x.title === 'The Long Winter'));

  assert.equal((await m.call('index_put', { title: 'X', index: 'nope' }, `&p=${p}`)).status, 400);
  assert.equal((await m.call('index', undefined, '&p=../etc')).status, 400);
});

test('scenes: saved against the version they were edited from', async () => {
  const m = await mac();
  const b = await browser();
  const p = newProject();
  await m.call('index_put', { title: 'Book', index: [] }, `&p=${p}`);
  const scene = (who, body) => who.call('scene_put', body, `&p=${p}&id=abc123`);

  // The Mac uploads it (new here: base 0).
  const first = await scene(m, { text: 'The ferry had gone.', base: 0 });
  assert.deepEqual(first.body, { version: 1, seq: 1 });

  // The iPad edits version 1.
  const fromIpad = await scene(b, { text: 'The last ferry had gone.', base: 1 });
  assert.equal(fromIpad.body.version, 2);

  // The Mac, still at version 1, tries to save its own edit: refused, with
  // what's there now, so nothing is overwritten.
  const clash = await scene(m, { text: 'The ferry was gone.', base: 1 });
  assert.equal(clash.status, 409);
  assert.deepEqual(clash.body.current, { text: 'The last ferry had gone.', version: 2 });
  assert.equal((await m.call('scene', undefined, `&p=${p}&id=abc123`)).body.text, 'The last ferry had gone.');

  // Saving the same text again changes nothing.
  assert.deepEqual((await scene(b, { text: 'The last ferry had gone.', base: 2 })).body, { version: 2, seq: 2 });

  // A new scene can't be "created" over one that exists.
  assert.equal((await scene(b, { text: 'x', base: 0 })).status, 409);
  assert.equal((await scene(b, { text: 'no base' })).status, 400);
});

test('changes since a sync point, and who made them', async () => {
  const m = await mac();
  const b = await browser();
  const p = newProject();
  await m.call('index_put', { title: 'Book', index: [] }, `&p=${p}`);
  await m.call('scene_put', { text: 'one', base: 0 }, `&p=${p}&id=s1`);
  await m.call('scene_put', { text: 'two', base: 0 }, `&p=${p}&id=s2`);
  const all = (await m.call('changes', undefined, `&p=${p}&since=0`)).body;
  assert.equal(all.seq, 2);
  assert.deepEqual(all.scenes.map(s => [s.id, s.text, s.by]), [['s1', 'one', 'mac'], ['s2', 'two', 'mac']]);

  await b.call('scene_put', { text: 'two, from the iPad', base: 1 }, `&p=${p}&id=s2`);
  const later = (await m.call('changes', undefined, `&p=${p}&since=${all.seq}`)).body;
  assert.equal(later.seq, 3);
  assert.deepEqual(later.scenes.map(s => [s.id, s.text, s.version, s.by]), [['s2', 'two, from the iPad', 2, 'web']]);
  assert.deepEqual((await m.call('changes', undefined, `&p=${p}&since=3`)).body.scenes, []);
});

test('every earlier version is kept', async () => {
  const m = await mac();
  const p = newProject();
  await m.call('index_put', { title: 'Book', index: [] }, `&p=${p}`);
  for (const [base, text] of [[0, 'first draft'], [1, 'second draft here'], [2, 'third']]) {
    await m.call('scene_put', { text, base }, `&p=${p}&id=s1`);
  }
  const versions = (await m.call('history', undefined, `&p=${p}&id=s1`)).body;
  assert.deepEqual(versions.map(v => [v.version, v.words]), [[2, 3], [1, 2]]);
  assert.equal((await m.call('history_text', undefined, `&p=${p}&id=s1&version=1`)).body.text, 'first draft');
  assert.equal((await m.call('history_text', undefined, `&p=${p}&id=s1&version=9`)).status, 404);
});

test('the inbox: scenes written on the iPad, until the Mac collects them', async () => {
  const m = await mac();
  const b = await browser();
  const p = newProject();
  await m.call('index_put', { title: 'Book', index: [] }, `&p=${p}`);
  const { body: { id } } = await b.call('inbox_add', { title: 'On the train', text: 'An idea.' }, `&p=${p}`);
  await b.call('inbox_add', { title: '', text: 'Another.' }, `&p=${p}`);

  const waiting = (await m.call('inbox', undefined, `&p=${p}`)).body;
  assert.deepEqual(waiting.map(i => [i.title, i.text]), [['On the train', 'An idea.'], ['Untitled', 'Another.']]);
  await m.call('inbox_collect', { ids: [id] }, `&p=${p}`);
  assert.deepEqual((await m.call('inbox', undefined, `&p=${p}`)).body.map(i => i.title), ['Untitled']);
});

test("its tables are prefixed, so it can share a database with other sites", async () => {
  await browser(); // makes sure the tables exist
  const tables = execSync(`${process.env.HELPER_DB} -N -e "SHOW TABLES"`).toString().trim().split('\n');
  assert.ok(tables.length >= 8);
  assert.ok(tables.every(t => t.startsWith('writer_')), tables.join(', '));
});
