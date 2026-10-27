// `status` results (status.schema.json v2) and layouts for the UI tests.

export const ME = 'a'.repeat(32);
export const STUDIO = 'b'.repeat(32);
export const WINDOWS = 'c'.repeat(32);
export const LINUX = 'd'.repeat(32);
export const OLD = 'e'.repeat(32);
export const TOO_OLD = 'f'.repeat(32);

const ME_COMPUTER = {
  id: ME,
  name: 'Desk Mac',
  os: 'macos',
  os_version: '15.4',
  version: '0.3.0',
  mode: 'installed',
  connection: 'self',
  rtt_ms: null,
  last_seen: null,
  permissions: { accessibility: 'granted', input_monitoring: 'granted' },
  limitations: [],
  displays: 2,
};

const peer = (id, name, os, extra = {}) => ({
  id,
  name,
  os,
  version: '0.3.0',
  connection: 'connected',
  rtt_ms: 8.707351000000001,
  last_seen: '2026-10-27T10:00:00Z',
  permissions: {},
  limitations: [],
  displays: 1,
  ...extra,
});

export const STUDIO_MAC = peer(STUDIO, 'Studio Mac', 'macos', { os_version: '15.4' });
export const WINDOWS_PC = peer(WINDOWS, 'Win PC', 'windows', {
  os_version: '11',
  mode: 'installed',
  limitations: ["Mouser cannot send Ctrl-Alt-Del to this computer's policy-locked screen."],
});
export const LINUX_BOX = peer(LINUX, 'Linux Box', 'linux', {
  limitations: ["Mouser cannot control this computer's login screen."],
});
export const OLD_PEER = peer(OLD, 'Old Mac', 'macos', { version: '0.2.8' });
export const INCOMPATIBLE_PEER = peer(TOO_OLD, 'Ancient PC', 'windows', {
  version: '0.1.0',
  connection: 'incompatible',
  rtt_ms: null,
});

export const problem = (id, message, fix = null, computer = ME, extra = {}) => ({
  problem_id: id,
  kind: 'missing_permission',
  computer,
  message,
  app: null,
  fix,
  raised_at: '2026-10-21T00:00:00Z',
  ...extra,
});

const connectionOf = (computers) => {
  const others = computers.filter((c) => c.connection !== 'self');
  if (others.length === 0) return 'alone';
  return others.every((c) => c.connection === 'connected') ? 'connected' : 'disconnected';
};

export function makeStatus({
  permissions = null,
  desk = null,
  computers = [],
  problems = [],
  active = ME,
  state = 'active',
  logging = { enabled: false, path: '' },
} = {}) {
  const all = [{ ...ME_COMPUTER, ...(permissions ? { permissions } : {}) }, ...computers];
  return {
    version: '0.3.0',
    computer: { id: ME, name: 'Desk Mac' },
    state,
    desk,
    active_computer: active,
    tray_summary: {
      state,
      connection: connectionOf(all),
      computers: all.length,
      displays: all.reduce((sum, c) => sum + c.displays, 0),
    },
    computers: all,
    problems,
    logging,
  };
}

const DESK = { id: '1'.repeat(32), name: 'Home office' };

export const STATUS_FIXTURES = {
  alone: makeStatus({ desk: DESK }),
  twoMacsWindowsLinux: makeStatus({ desk: DESK, computers: [STUDIO_MAC, WINDOWS_PC, LINUX_BOX], active: STUDIO }),
  oldPeer: makeStatus({ desk: DESK, computers: [STUDIO_MAC, OLD_PEER] }),
  incompatiblePeer: makeStatus({
    desk: DESK,
    computers: [STUDIO_MAC, INCOMPATIBLE_PEER],
    problems: [
      problem(
        `incompatible_version:${TOO_OLD}`,
        'Ancient PC runs Mouser 0.1.0 and cannot work with this version. Update Ancient PC.',
        `update_computer:${TOO_OLD}`,
        TOO_OLD,
        { kind: 'incompatible_version' },
      ),
    ],
  }),
  elevatedApp: makeStatus({
    desk: DESK,
    computers: [STUDIO_MAC, WINDOWS_PC],
    problems: [
      problem(
        `elevated_app:${WINDOWS}`,
        'Portable Mouser cannot control VMware Workstation, which runs as administrator on Win PC.',
        `restart_elevated:${WINDOWS}`,
        WINDOWS,
        { kind: 'elevated_app', app: 'VMware Workstation' },
      ),
    ],
  }),
};

// A `layout.get` result for two Macs (Desk Mac with two displays), a Windows PC and a Linux box.
export const display = (computer, key, name, computerName, extra = {}) => ({
  display_id: { computer, key },
  name,
  pixel_size: [2560, 1440],
  logical_size: [1280, 720],
  scale: 2,
  physical_size_mm: [600, 340],
  origin_local: [0, 0],
  included: true,
  needs_review: false,
  computer_name: computerName,
  online: true,
  last_seen: null,
  ...extra,
});

export function makeLayout({ review = [] } = {}) {
  const displays = [
    display(ME, '1', 'Built-in', 'Desk Mac', { needs_review: review.includes(ME) }),
    display(STUDIO, '1', 'Studio Display', 'Studio Mac', { needs_review: review.includes(STUDIO) }),
    display(WINDOWS, '1', 'DELL U2720Q', 'Win PC', { needs_review: review.includes(WINDOWS) }),
    display(LINUX, '1', 'DP-1', 'Linux Box', { needs_review: review.includes(LINUX) }),
  ];
  return {
    placements: displays.map((d, i) => ({ display_id: d.display_id, x: i * 600, y: 0 })),
    edge_options: [],
    equalize_dimensions: false,
    block_fullscreen: true,
    displays,
  };
}
