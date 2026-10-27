// Consumer side of contracts/ui-navigation.md: sidebar, routes and mouser://navigate.
import assert from 'node:assert/strict';
import test from 'node:test';
import { SIDEBAR, init, needsSetup } from './nav.js';
import { click, fakeApi, flush, loadPage } from './test_helpers.js';
import { STATUS_FIXTURES, makeStatus, problem } from './status_fixture.js';

const IDS = ['desk', 'displays', 'health', 'settings', 'diagnostics', 'help', 'welcome'];

// Fake sections that record what the window tells them.
function fakeSections(log) {
  return Object.fromEntries(
    IDS.map((id) => [
      id,
      {
        create: ({ doc, navigate }) => {
          const element = doc.createElement('div');
          log.navigate[id] = navigate;
          return {
            element,
            update: (status) => log.updates.push([id, status.version]),
            note: (note) => log.notes.push([id, note.method]),
            open: (param) => log.opened.push([id, param]),
          };
        },
      },
    ]),
  );
}

async function start({ hash = '', status = STATUS_FIXTURES.twoMacsWindowsLinux } = {}) {
  const win = loadPage('index.html');
  win.location.hash = hash;
  const api = fakeApi({ status });
  const log = { navigate: {}, updates: [], notes: [], opened: [] };
  const app = await init({ doc: win.document, api, sections: fakeSections(log), onFocus: (h) => log.focus = h });
  await flush();
  const doc = win.document;
  const visible = () => IDS.filter((id) => !doc.getElementById(`section-${id}`).hidden);
  return { win, doc, api, log, app, visible };
}

test('the sidebar shows "Your desk" with Desk, Displays, Health; "Configure" with Settings; "Support" with Diagnostics and Help', async () => {
  const { doc } = await start();
  const groups = [...doc.querySelectorAll('#nav section')].map((group) => [
    group.querySelector('h2').textContent,
    [...group.querySelectorAll('a')].map((a) => a.textContent),
  ]);
  assert.deepEqual(groups, [
    ['Your desk', ['Desk', 'Displays', 'Health']],
    ['Configure', ['Settings']],
    ['Support', ['Diagnostics', 'Help']],
  ]);
  assert.deepEqual(
    SIDEBAR.map((g) => g.heading),
    ['Your desk', 'Configure', 'Support'],
  );
  assert.ok(doc.getElementById('search'), 'the search field is at the top of the sidebar');
});

test('the window opens on Desk and shows one section at a time', async () => {
  const { doc, visible } = await start();
  assert.deepEqual(visible(), ['desk']);
  assert.equal(doc.getElementById('title').textContent, 'Desk');
  assert.equal(doc.querySelector('#nav a[aria-current="page"]').textContent, 'Desk');
});

test('choosing a sidebar item shows that section in the same window', async () => {
  const { win, doc, visible } = await start();
  click(win, [...doc.querySelectorAll('#nav a')].find((a) => a.textContent === 'Displays'));
  await flush();
  assert.equal(win.location.hash, '#displays');
  assert.deepEqual(visible(), ['displays']);
  assert.equal(doc.getElementById('title').textContent, 'Displays');
});

test('#health/<id> opens Health at that computer and #settings/<key> at that control', async () => {
  const { win, log, visible } = await start({ hash: '#health/abc' });
  assert.deepEqual(visible(), ['health']);
  assert.deepEqual(log.opened.at(-1), ['health', 'abc']);
  win.location.hash = '#settings/advanced.port';
  await flush();
  assert.deepEqual(visible(), ['settings']);
  assert.deepEqual(log.opened.at(-1), ['settings', 'advanced.port']);
  win.location.hash = '#help';
  await flush();
  assert.deepEqual(visible(), ['help']);
  assert.deepEqual(log.opened.at(-1), ['settings', 'advanced.port'], 'a route without a part opens nothing');
});

test('a mouser://navigate event switches the section in place, also to the one already shown', async () => {
  const { win, api, visible, log } = await start();
  api.emitNavigate('#diagnostics');
  await flush();
  assert.deepEqual(visible(), ['diagnostics']);
  assert.equal(win.location.hash, '#diagnostics');
  api.emitNavigate('#health/xyz');
  api.emitNavigate('#health/xyz');
  await flush();
  assert.deepEqual(log.opened.filter(([id]) => id === 'health'), [['health', 'xyz'], ['health', 'xyz']]);
});

test('#welcome hides the sidebar, and finishing goes to #desk with the sidebar back', async () => {
  const { win, doc, log, visible } = await start({ hash: '#welcome' });
  assert.deepEqual(visible(), ['welcome']);
  assert.equal(doc.getElementById('sidebar').hidden, true);
  log.navigate.welcome('#desk');
  await flush();
  assert.equal(win.location.hash, '#desk');
  assert.deepEqual(visible(), ['desk']);
  assert.equal(doc.getElementById('sidebar').hidden, false);
});

test('without a route the window opens on #welcome while setup is unfinished, and on Desk after', async () => {
  const fresh = await start({ status: makeStatus() });
  assert.deepEqual(fresh.visible(), ['welcome']);
  const missing = await start({
    status: makeStatus({ desk: STATUS_FIXTURES.alone.desk, permissions: { accessibility: 'missing' } }),
  });
  assert.deepEqual(missing.visible(), ['welcome']);
  const done = await start({ status: STATUS_FIXTURES.alone });
  assert.deepEqual(done.visible(), ['desk']);
  assert.equal(needsSetup(STATUS_FIXTURES.alone), false);
  assert.equal(needsSetup(makeStatus()), true);
});

test('one subscription feeds every section: the first status, then each status or problem notification and each focus', async () => {
  const { api, log } = await start();
  assert.equal(log.updates.length, IDS.length);
  api.emit('status', {});
  await flush();
  assert.equal(log.updates.length, IDS.length * 2);
  api.emit('problem', problem('x', 'm'));
  await flush();
  assert.equal(log.updates.length, IDS.length * 3);
  api.emit('config', {});
  await flush();
  assert.equal(log.updates.length, IDS.length * 3, 'other topics do not refetch');
  log.focus();
  await flush();
  assert.equal(log.updates.length, IDS.length * 4);
  assert.equal(api.calls.filter(([m]) => m === 'status').length, 4);
  assert.deepEqual(log.notes.filter(([id]) => id === 'desk').map(([, m]) => m), ['status', 'problem', 'config']);
});

test('no section polls: only notifications, focus and navigation cause status calls', async () => {
  const { api } = await start();
  await flush();
  assert.equal(api.calls.filter(([m]) => m === 'status').length, 1);
});

test('a status call that fails is shown in the window instead of being lost', async () => {
  const win = loadPage('index.html');
  win.location.hash = '#desk';
  const api = fakeApi({ status: STATUS_FIXTURES.alone });
  const log = { navigate: {}, updates: [], notes: [], opened: [] };
  await init({ doc: win.document, api, sections: fakeSections(log), onFocus: (h) => (log.focus = h) });
  api.call = async () => {
    throw new Error('Mouser is not answering.');
  };
  log.focus();
  await flush();
  assert.equal(win.document.getElementById('error').textContent, 'Mouser is not answering.');
});
