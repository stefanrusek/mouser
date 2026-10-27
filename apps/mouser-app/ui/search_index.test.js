import assert from 'node:assert/strict';
import test from 'node:test';
import { SIDEBAR, init } from './nav.js';
import { ENTRIES, search } from './search_index.js';
import { REGISTRY } from './sections/registry.js';
import { click, fakeApi, flush, loadPage } from './test_helpers.js';
import { STATUS_FIXTURES, makeLayout } from './status_fixture.js';

const sidebarIds = SIDEBAR.flatMap((group) => group.items.map((item) => item.id));

test('entries are {route, title, section, keywords[]}', () => {
  for (const entry of ENTRIES) {
    assert.deepEqual(Object.keys(entry).sort(), ['keywords', 'route', 'section', 'title']);
    assert.match(entry.route, /^#[a-z]+(\/[A-Za-z0-9_.-]+)?$/);
    assert.ok(Array.isArray(entry.keywords));
    assert.ok(entry.title && entry.section);
  }
});

test('there is one entry per section, with the sidebar group as its section', () => {
  for (const group of SIDEBAR) {
    for (const item of group.items) {
      const entry = ENTRIES.find((e) => e.route === `#${item.id}`);
      assert.equal(entry.title, item.title);
      assert.equal(entry.section, group.heading);
    }
  }
});

test('matching is a case-insensitive substring of the title or a keyword', () => {
  const routes = (query) => search(query).map((e) => e.route);
  assert.ok(routes('NETWORK PORT').includes('#settings/advanced.port'));
  assert.ok(routes('thumb').includes('#settings/mouse.buttons_4_5'));
  assert.ok(routes('Diag').includes('#diagnostics'));
  assert.deepEqual(routes('zzzz-no-such-thing'), []);
  assert.deepEqual(routes(''), []);
  assert.deepEqual(routes('   '), []);
});

test('every route is a section, or a control of a section, and nothing else', () => {
  for (const { route } of ENTRIES) {
    const [section] = route.slice(1).split('/');
    assert.ok(sidebarIds.includes(section), route);
  }
});

test('every config key and setting control reachable in the UI has an entry, one sidebar click and one control away (SC-005)', async () => {
  const status = STATUS_FIXTURES.twoMacsWindowsLinux;
  const api = fakeApi({ status, 'layout.get': makeLayout(), 'desks.discovered': [], 'join.pending': [], 'config.get': (p) => ({ 'advanced.allowlist': [], 'advanced.port': 47801 })[p.key] ?? 'passthrough', transfers: [] });
  const win = loadPage('index.html');
  const controls = [];
  for (const [id, factory] of Object.entries(REGISTRY)) {
    const section = factory.create({ doc: win.document, api, navigate: () => {}, fetch: async () => ({ ok: true, text: async () => '' }) });
    section.update(status);
    await flush();
    for (const node of section.element.querySelectorAll('[data-setting]')) {
      controls.push(`#${id}/${node.getAttribute('data-setting')}`);
    }
  }
  assert.ok(controls.length > 0);
  const routes = ENTRIES.map((e) => e.route);
  for (const control of controls) assert.ok(routes.includes(control), `${control} has no search entry`);
  for (const route of routes.filter((r) => r.includes('/'))) assert.ok(controls.includes(route), `${route} names no control`);
});

test('typing lists the matching sections and settings, and choosing one opens it', async () => {
  const win = loadPage('index.html');
  const doc = win.document;
  const opened = [];
  const sections = Object.fromEntries(
    sidebarIds.concat('welcome').map((id) => [
      id,
      {
        create: ({ doc: d }) => {
          const element = d.createElement('div');
          return { element, update() {}, open: (param) => opened.push([id, param]) };
        },
      },
    ]),
  );
  await init({ doc, api: fakeApi({ status: STATUS_FIXTURES.alone }), onFocus: () => {}, sections });
  const box = doc.getElementById('search');
  const results = doc.getElementById('results');
  assert.equal(results.hidden, true);
  box.value = 'port';
  box.dispatchEvent(new win.Event('input', { bubbles: true }));
  const items = [...results.querySelectorAll('button')];
  assert.ok(items.length > 0);
  assert.ok(items.some((b) => b.textContent.includes('Network port')));
  click(win, items.find((b) => b.textContent.includes('Network port')));
  assert.equal(win.location.hash, '#settings/advanced.port');
  assert.deepEqual(opened.at(-1), ['settings', 'advanced.port']);
  assert.equal(box.value, '');
  assert.equal(results.hidden, true);
});

test('with nothing matching it says so, and Enter chooses the first result', async () => {
  const win = loadPage('index.html');
  const doc = win.document;
  const sections = Object.fromEntries(
    sidebarIds.concat('welcome').map((id) => [id, { create: ({ doc: d }) => ({ element: d.createElement('div'), update() {} }) }]),
  );
  await init({ doc, api: fakeApi({ status: STATUS_FIXTURES.alone }), onFocus: () => {}, sections });
  const box = doc.getElementById('search');
  box.value = 'qqqq';
  box.dispatchEvent(new win.Event('input', { bubbles: true }));
  assert.equal(doc.getElementById('results').textContent, 'Nothing matches.');
  box.dispatchEvent(new win.KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  assert.equal(win.location.hash, '', 'no result, so Enter goes nowhere');
  box.value = 'diagnostics';
  box.dispatchEvent(new win.Event('input', { bubbles: true }));
  box.dispatchEvent(new win.KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  assert.equal(doc.getElementById('results').hidden, true);
  box.dispatchEvent(new win.Event('input', { bubbles: true }));
  box.dispatchEvent(new win.KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  assert.equal(win.location.hash, '#diagnostics');
});
