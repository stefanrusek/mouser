// Acceptance tests for 002 Story 3, "one Mouser window where I can find everything":
// the real window (index.html with every section) over fixtures. Each test is
// named with its scenario's Given/When/Then text from spec.md.
import assert from 'node:assert/strict';
import test from 'node:test';
import { init } from './nav.js';
import { REGISTRY } from './sections/registry.js';
import { click, fakeApi, flush, labelled, loadPage } from './test_helpers.js';
import {
  ME, LINUX, STATUS_FIXTURES, STUDIO, WINDOWS, makeLayout, makeStatus, problem,
} from './status_fixture.js';

const GUIDE = '# Troubleshooting\n\nStart with `mouser status`.';
const CONFIG = { 'advanced.allowlist': [], 'advanced.port': 47801, 'mouse.buttons_4_5': 'passthrough' };

async function openWindow({ status = STATUS_FIXTURES.twoMacsWindowsLinux, layout = makeLayout(), hash = '', answers = {} } = {}) {
  const win = loadPage('index.html');
  win.fetch = async () => ({ ok: true, text: async () => GUIDE });
  win.location.hash = hash;
  const api = fakeApi({
    status,
    'layout.get': layout,
    'layout.set': (p) => ({ ...layout, ...p }),
    'layout.keep_placement': {},
    'desks.discovered': [],
    'join.pending': [],
    transfers: [],
    'config.get': (p) => CONFIG[p.key],
    portable: false,
    collect_logs: {
      path: '/Downloads/mouser-logs-Home-office.zip',
      computers: [{ id: ME, name: 'Desk Mac', included: true }, { id: STUDIO, name: 'Studio Mac', included: false, reason: 'offline' }],
    },
    ...answers,
  });
  await init({ doc: win.document, api, onFocus: () => {}, sections: REGISTRY });
  await flush();
  await flush();
  const doc = win.document;
  const shown = () => [...doc.querySelectorAll('#sections > div')].filter((n) => !n.hidden).map((n) => n.id.replace('section-', ''));
  return { win, doc, api, shown };
}
const entryOf = (doc, id) => doc.querySelector(`#section-health [data-computer="${id}"]`);

test('Given Mouser is running, When the desk owner opens Mouser, Then one window opens with a search field and a sidebar whose sections are grouped under headings: "Your desk" with Desk, Displays and Health; "Configure" with Settings; "Support" with Diagnostics and Help. Choosing a section shows it in the same window, and the window opens on Desk.', async () => {
  const { win, doc, shown } = await openWindow();
  assert.ok(doc.getElementById('search'));
  assert.deepEqual(
    [...doc.querySelectorAll('#nav section')].map((g) => [g.querySelector('h2').textContent, [...g.querySelectorAll('a')].map((a) => a.textContent)]),
    [['Your desk', ['Desk', 'Displays', 'Health']], ['Configure', ['Settings']], ['Support', ['Diagnostics', 'Help']]],
  );
  assert.deepEqual(shown(), ['desk']);
  click(win, [...doc.querySelectorAll('#nav a')].find((a) => a.textContent === 'Health'));
  assert.deepEqual(shown(), ['health']);
  assert.equal(doc.querySelectorAll('.shell').length, 1);
});

test('Given the window is open, When the desk owner chooses a tray-menu item that leads to a part of Mouser (for example arranging displays), Then the same window comes to the front at that section, and no second Mouser window opens.', async () => {
  const { api, doc, shown } = await openWindow();
  api.emitNavigate('#displays');
  await flush();
  assert.deepEqual(shown(), ['displays']);
  api.emitNavigate('#health/' + WINDOWS);
  await flush();
  assert.deepEqual(shown(), ['health']);
  assert.ok(entryOf(doc, WINDOWS).classList.contains('highlight'));
  assert.equal(doc.querySelectorAll('.shell').length, 1);
});

test('Given the window is open, When the desk owner types in the search field, Then matching sections and individual settings are listed, and choosing one opens it.', async () => {
  const { win, doc, shown } = await openWindow();
  const box = doc.getElementById('search');
  box.value = 'dis';
  box.dispatchEvent(new win.Event('input', { bubbles: true }));
  const found = [...doc.querySelectorAll('#results button')].map((b) => b.textContent);
  assert.ok(found.some((t) => t.startsWith('Displays')), 'a section');
  assert.ok(found.some((t) => t.startsWith('Equalize display dimensions')), 'an individual setting');
  click(win, [...doc.querySelectorAll('#results button')].find((b) => b.textContent.startsWith('Equalize')));
  assert.deepEqual(shown(), ['displays']);
  assert.equal(win.location.hash, '#displays/equalize_dimensions');
  assert.equal(doc.activeElement.getAttribute('data-setting'), 'equalize_dimensions');
});

test('Given a problem the desk owner can act on is raised for a computer, When the owner opens Health, Then the problem appears once, in that computer\'s entry and no other, with its fix as one action where a fix exists.', async () => {
  const { doc } = await openWindow({ status: STATUS_FIXTURES.elevatedApp, hash: '#health' });
  const problems = [...doc.querySelectorAll('#section-health .problem')];
  assert.equal(problems.length, 1);
  assert.equal(problems[0].closest('[data-computer]').getAttribute('data-computer'), WINDOWS);
  assert.deepEqual([...problems[0].querySelectorAll('button')].map((b) => b.textContent), ['Restart Mouser as administrator']);
});

test('Given a computer has platform limitations (001 FR-001), When the desk owner opens Health, Then they are listed once, under "What doesn\'t work here" in that computer\'s entry, and in no other computer\'s entry and not as a problem notice.', async () => {
  const { doc } = await openWindow({ hash: '#health' });
  const limitation = "Mouser cannot control this computer's login screen.";
  const where = [...doc.querySelectorAll('#section-health .limitation')].filter((n) => n.textContent === limitation);
  assert.equal(where.length, 1);
  assert.equal(where[0].closest('[data-computer]').getAttribute('data-computer'), LINUX);
  assert.equal(where[0].closest('.limitations').querySelector('h3').textContent, "What doesn't work here");
  assert.equal(doc.querySelectorAll('#section-health .problem').length, 0);
});

test('Given Health shows how responsive a computer is, When the desk owner reads it, Then the time is in whole milliseconds (for example "Responds quickly (9 ms)").', async () => {
  const { doc } = await openWindow({ hash: '#health' });
  assert.equal(entryOf(doc, STUDIO).querySelector('.rtt').textContent, 'Responds quickly (9 ms)');
});

test('Given Mouser is launched for the first time, When the window opens, Then the first-run steps of 001 Story 1 scenario 1 and the desk step run inside the window, and afterwards Health shows this computer\'s permissions, each with whether it is granted and one action that opens the system screen to grant it.', async () => {
  const fresh = makeStatus({ permissions: { accessibility: 'missing', input_monitoring: 'granted' } });
  const { win, doc, api, shown } = await openWindow({
    status: fresh,
    answers: { 'desk.create': { desk_id: 'd', name: 'Home office' }, 'desks.discovered': [] },
  });
  assert.deepEqual(shown(), ['welcome']);
  assert.equal(doc.getElementById('sidebar').hidden, true);
  assert.match(doc.querySelector('#section-welcome .permission-list').textContent, /Accessibility[\s\S]*Missing/);
  click(win, doc.querySelector('#section-welcome .continue'));
  await flush();
  doc.querySelector('#section-welcome .desk-name').value = 'Home office';
  doc.querySelector('#section-welcome .create-form').dispatchEvent(new win.Event('submit', { cancelable: true }));
  await flush();
  click(win, doc.querySelector('#section-welcome .result-action'));
  assert.deepEqual(shown(), ['desk']);
  assert.equal(doc.getElementById('sidebar').hidden, false);

  click(win, [...doc.querySelectorAll('#nav a')].find((a) => a.textContent === 'Health'));
  const mine = entryOf(doc, ME);
  assert.deepEqual([...mine.querySelectorAll('.permission')].map((n) => n.textContent), ['Accessibility: Missing Open settings', 'Input Monitoring: Granted']);
  click(win, labelled(mine, 'Open settings'));
  await flush();
  assert.deepEqual(api.calls.at(-1), ['open_permission_settings', 'accessibility']);
});

test('Given the Displays section is shown, When the desk owner looks at the arranger, Then every display\'s computer name and display name can be read in full, on the display or beside it when the display is drawn too small.', async () => {
  const layout = makeLayout();
  layout.displays[3].physical_size_mm = [50, 30];
  const { doc } = await openWindow({ layout, hash: '#displays' });
  for (const display of layout.displays) {
    const full = `${display.computer_name}: ${display.name}`;
    const tile = [...doc.querySelectorAll('#section-displays .tile')].find((t) => t.getAttribute('aria-label') === full);
    const beside = [...doc.querySelectorAll('#section-displays .external-label')].find((l) => l.textContent === full);
    assert.ok(tile, full);
    assert.equal(Boolean(beside), tile.classList.contains('small'), `${full}: beside the display exactly when it is drawn too small`);
    if (!beside) assert.equal(tile.textContent, `${display.computer_name}${display.name}`);
  }
});

test('Given a computer\'s displays were placed automatically (001 Story 2 scenario 1), When the desk owner views Displays, Then a message names the computer, says its displays were placed automatically and should be checked against the real desk, and offers to keep the placement; the message goes away when the owner keeps or moves the placement.', async () => {
  let kept = false;
  const flagged = () => makeLayout({ review: kept ? [] : [STUDIO] });
  const { win, doc } = await openWindow({
    hash: '#displays',
    answers: { 'layout.get': () => flagged(), 'layout.keep_placement': () => { kept = true; return {}; } },
  });
  const message = doc.querySelector('#section-displays .review-message');
  assert.match(message.textContent, /Studio Mac's displays were placed automatically\. Check that they match your real desk/);
  click(win, labelled(message, 'Keep this placement'));
  await flush();
  assert.equal(doc.querySelectorAll('#section-displays .review-message').length, 0);
});

test('Given the Diagnostics section is shown, When the desk owner uses it, Then it offers the diagnostic logging switch, opening the log\'s location, and "Collect logs from the desk", which produces the bundle of FR-007 and shows it in the file manager.', async () => {
  const { win, doc, api } = await openWindow({ hash: '#diagnostics', status: makeStatus({ desk: STATUS_FIXTURES.alone.desk, logging: { enabled: true, path: '/logs/mouser.log' } }) });
  const section = doc.getElementById('section-diagnostics');
  assert.ok(section.querySelector('[data-setting="logging"]'));
  click(win, labelled(section, 'Open log location'));
  click(win, labelled(section, 'Collect logs from the desk'));
  await flush();
  assert.deepEqual(api.calls.slice(-3), [
    ['reveal_in_file_manager', '/logs/mouser.log'],
    ['collect_logs'],
    ['reveal_in_file_manager', '/Downloads/mouser-logs-Home-office.zip'],
  ]);
  assert.match(section.querySelector('.collected').textContent, /Studio Mac: not included \(offline\)/);
});

test('Given Mouser is running, When the desk owner left-clicks the menu-bar or tray icon, Then Mouser pauses (or resumes) as in 001 Story 2 scenario 12, and the icon shows the new state; right-clicking the icon opens the menu.', async () => {
  // The click and the icon are the tray's (tray_tests.rs); what the window shows is the new state.
  const { api, doc } = await openWindow();
  assert.equal(doc.querySelector('#section-desk .engine-state').textContent, 'Mouser is active');
  api.call = async () => makeStatus({ desk: STATUS_FIXTURES.alone.desk, state: 'paused' });
  api.emit('status', {});
  await flush();
  await flush();
  assert.equal(doc.querySelector('#section-desk .engine-state').textContent, 'Mouser is paused');
});

test('Given this computer is on a desk with other computers, When the desk owner looks at the menu-bar or tray icon, Then it shows whether this computer is connected to its desk-mates or unexpectedly disconnected from them (for example a green dot when connected and a red dot when unexpectedly disconnected), using a difference in shape or symbol as well as colour, alongside the active or paused state of 001 Story 1 scenario 10.', async () => {
  // The icon is drawn from tray_summary.connection (tray_tests.rs, tray_icons_tests.rs); the window agrees with it.
  const connected = STATUS_FIXTURES.twoMacsWindowsLinux;
  assert.equal(connected.tray_summary.connection, 'connected');
  const offline = makeStatus({
    desk: connected.desk,
    computers: [{ ...connected.computers[1], connection: 'offline' }],
  });
  assert.equal(offline.tray_summary.connection, 'disconnected');
  const { doc } = await openWindow({ status: offline });
  assert.equal(doc.querySelector('#section-desk [data-computer="' + STUDIO + '"] .connection').textContent, 'Offline');
});

test('Given this computer is the only computer on its desk, or is on no desk, or the desk owner removed the other computers or left the desk, When the desk owner looks at the menu-bar or tray icon, Then it does not show the unexpectedly-disconnected state.', async () => {
  for (const status of [STATUS_FIXTURES.alone, makeStatus()]) {
    assert.equal(status.tray_summary.connection, 'alone');
    const { doc } = await openWindow({ status, hash: '#desk' });
    assert.equal(doc.querySelectorAll('#section-desk .connection').length, 1);
    assert.equal(doc.querySelector('#section-desk .connection').textContent, 'This computer');
  }
});

test('Given the Help section is shown, When the desk owner opens it, Then it shows this computer\'s Mouser version and the troubleshooting guide shipped with Mouser.', async () => {
  const { doc } = await openWindow({ hash: '#help' });
  const help = doc.getElementById('section-help');
  assert.equal(help.querySelector('.version').textContent, 'Mouser 0.3.0');
  assert.equal(help.querySelector('h1').textContent, 'Troubleshooting');
  assert.equal(help.querySelector('code').textContent, 'mouser status');
});
