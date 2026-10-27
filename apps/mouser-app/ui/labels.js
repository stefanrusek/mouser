// The texts every section uses for connections, permissions and systems.

export const CONNECTION_TEXT = {
  self: 'This computer',
  connected: 'Connected',
  reconnecting: 'Reconnecting',
  offline: 'Offline',
  incompatible: 'Needs an update',
};

export const PERMISSIONS = {
  accessibility: {
    title: 'Accessibility',
    why: 'Lets Mouser move the pointer and type on this computer.',
  },
  input_monitoring: {
    title: 'Input Monitoring',
    why: 'Lets Mouser see your mouse and keyboard on this computer.',
  },
  screen_recording: {
    title: 'Screen Recording',
    why: 'Lets Mouser see where full-screen windows are.',
  },
  full_disk_access: {
    title: 'Full Disk Access',
    why: 'Lets Mouser copy files to and from this computer.',
  },
  input_capture_portal: {
    title: 'Input capture',
    why: "Lets Mouser capture your mouse and keyboard through the desktop's input portal.",
  },
  remote_desktop_portal: {
    title: 'Remote control',
    why: "Lets Mouser control this computer through the desktop's remote-control portal.",
  },
};

export const permissionLabel = (kind) => PERMISSIONS[kind].title;

export const OS_NAMES = { macos: 'macOS', windows: 'Windows', linux: 'Linux' };
