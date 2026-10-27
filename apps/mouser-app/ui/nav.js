// The one window: a sidebar of sections grouped under headings, a search
// field, and the section that the location hash names (contracts/ui-navigation.md).

import { el, guarded } from './dom.js';
import { search } from './search_index.js';

export const SIDEBAR = [
  { heading: 'Your desk', items: [{ id: 'desk', title: 'Desk' }, { id: 'displays', title: 'Displays' }, { id: 'health', title: 'Health' }] },
  { heading: 'Configure', items: [{ id: 'settings', title: 'Settings' }] },
  { heading: 'Support', items: [{ id: 'diagnostics', title: 'Diagnostics' }, { id: 'help', title: 'Help' }] },
];

const TITLES = Object.fromEntries([
  ...SIDEBAR.flatMap((group) => group.items.map((item) => [item.id, item.title])),
  ['welcome', 'Welcome to Mouser'],
]);

// Setup is unfinished while this computer is off a desk or is missing a permission.
export const needsSetup = (status) =>
  !status.desk ||
  status.computers.some((c) => c.connection === 'self' && Object.values(c.permissions).includes('missing'));

// `sections` maps a section id to a module with `create({doc, api, navigate})`, which returns
// `{element, update(status), note?(notification), open?(part)}`.
export async function init({ doc, api, onFocus, sections }) {
  const win = doc.defaultView;
  const errorLine = doc.getElementById('error');
  const sidebar = doc.getElementById('sidebar');
  const container = doc.getElementById('sections');
  const box = doc.getElementById('search');
  const results = doc.getElementById('results');
  const state = { route: null };

  const made = Object.fromEntries(
    Object.entries(sections).map(([id, module]) => {
      const section = module.create({ doc, api, navigate: (route) => go(route) });
      section.element.id = `section-${id}`;
      section.element.hidden = true;
      container.append(section.element);
      return [id, section];
    }),
  );

  const links = {};
  doc.getElementById('nav').replaceChildren(
    ...SIDEBAR.map((group) =>
      el(
        doc,
        'section',
        {},
        el(doc, 'h2', {}, group.heading),
        ...group.items.map((item) => {
          const link = el(doc, 'a', { href: `#${item.id}` }, item.title);
          link.addEventListener('click', (event) => {
            event.preventDefault();
            go(`#${item.id}`);
          });
          links[item.id] = link;
          return link;
        }),
      ),
    ),
  );

  function show(route) {
    state.route = route;
    const [id, part] = route.slice(1).split('/');
    for (const [name, section] of Object.entries(made)) section.element.hidden = name !== id;
    for (const [name, link] of Object.entries(links)) {
      if (name === id) link.setAttribute('aria-current', 'page');
      else link.removeAttribute('aria-current');
    }
    sidebar.hidden = id === 'welcome';
    doc.getElementById('title').textContent = TITLES[id];
    if (part) made[id].open(part);
  }
  function go(route) {
    win.location.hash = route;
    show(route);
  }
  win.addEventListener('hashchange', () => {
    if (win.location.hash !== state.route) show(win.location.hash);
  });

  async function refresh() {
    const status = await api.call('status');
    for (const section of Object.values(made)) section.update(status);
    return status;
  }
  const tick = guarded(errorLine, refresh, { clear: false });

  function choose(entry) {
    box.value = '';
    results.hidden = true;
    go(entry.route);
  }
  function list() {
    const found = search(box.value);
    results.hidden = box.value.trim() === '';
    results.replaceChildren(
      ...(found.length === 0
        ? [el(doc, 'li', {}, 'Nothing matches.')]
        : found.map((entry) => {
            const pick = el(doc, 'button', { type: 'button' }, entry.title, el(doc, 'small', {}, entry.section));
            pick.addEventListener('click', () => choose(entry));
            return el(doc, 'li', {}, pick);
          })),
    );
    return found;
  }
  box.addEventListener('input', list);
  box.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') results.hidden = true;
    if (event.key === 'Enter') {
      const [first] = search(box.value);
      if (first) choose(first);
    }
  });

  await api.onNotification((note) => {
    for (const section of Object.values(made)) section.note?.(note);
    if (note.method === 'status' || note.method === 'problem') tick();
  });
  await api.onNavigate(({ route }) => go(route));
  onFocus(tick);
  const status = await refresh();
  const hash = win.location.hash;
  show(hash === '' ? (needsSetup(status) ? '#welcome' : '#desk') : hash);
  return { go, tick };
}
