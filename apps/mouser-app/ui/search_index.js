// What the search field can find: every section and every setting control.

const YOUR_DESK = 'Your desk';
const CONFIGURE = 'Configure';
const SUPPORT = 'Support';

export const ENTRIES = [
  { route: '#desk', title: 'Desk', section: YOUR_DESK, keywords: ['computers', 'members', 'remove', 'leave', 'move', 'join', 'transfer', 'files', 'overview'] },
  { route: '#displays', title: 'Displays', section: YOUR_DESK, keywords: ['arrange', 'arranger', 'screens', 'monitors', 'placement', 'layout'] },
  { route: '#displays/block_fullscreen', title: "Don't cross while an app is full screen", section: YOUR_DESK, keywords: ['fullscreen', 'games', 'video'] },
  { route: '#displays/equalize_dimensions', title: 'Equalize display dimensions', section: YOUR_DESK, keywords: ['sizes', 'scale', 'pointer'] },
  { route: '#displays/block_corners', title: 'Block crossing in corners', section: YOUR_DESK, keywords: ['edge', 'edges', 'corner'] },
  { route: '#health', title: 'Health', section: YOUR_DESK, keywords: ['problems', 'permissions', 'connection', 'limitations', 'responds', 'speed'] },
  { route: '#settings', title: 'Settings', section: CONFIGURE, keywords: ['preferences', 'options'] },
  { route: '#settings/mouse.buttons_4_5', title: 'Mouse buttons 4 and 5', section: CONFIGURE, keywords: ['back', 'forward', 'thumb', 'extra buttons'] },
  { route: '#settings/advanced.port', title: 'Network port', section: CONFIGURE, keywords: ['udp', 'firewall', 'advanced'] },
  { route: '#settings/advanced.allowlist', title: 'Computers allowed to join', section: CONFIGURE, keywords: ['allowlist', 'network range', 'advanced'] },
  { route: '#settings/login-screen', title: 'Control at the login screen', section: CONFIGURE, keywords: ['administrator', 'lock screen', 'macos'] },
  { route: '#diagnostics', title: 'Diagnostics', section: SUPPORT, keywords: ['logs', 'logging', 'bundle', 'collect'] },
  { route: '#diagnostics/logging', title: 'Diagnostic logging', section: SUPPORT, keywords: ['log', 'record', 'switch'] },
  { route: '#help', title: 'Help', section: SUPPORT, keywords: ['troubleshooting', 'guide', 'version', 'support'] },
];

// The entries whose title or keywords contain `query`, ignoring case.
export function search(query) {
  const needle = query.trim().toLowerCase();
  if (needle === '') return [];
  return ENTRIES.filter((entry) => [entry.title, ...entry.keywords].some((text) => text.toLowerCase().includes(needle)));
}
