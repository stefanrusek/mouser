// The UI's only way to reach the engine: Tauri commands and one event.

export const EVENT = 'mouser://notification';
export const LANDING_EVENT = 'mouser://landing';
export const QUICKJUMP_EVENT = 'mouser://quickjump';
export const NAVIGATE_EVENT = 'mouser://navigate';

export function makeApi(tauri) {
  const { invoke } = tauri.core;
  return {
    // Call a control-API method by name (contracts/control-api.md).
    call: (method, params = null) => invoke('control', { request: { method, params } }),
    openPermissionSettings: (kind) => invoke('open_permission_settings', { kind }),
    // Run the fix of an open problem (the engine acknowledges it; the app opens whatever screen it needs).
    fix: (problemId, fix) => invoke('fix', { problemId, fix }),
    // The latest `transfer` notification of every transfer so far, for a window that opened late.
    transfers: () => invoke('transfers'),
    // The problem id of the edge the arranger should open, once (null when none was asked for).
    takeArrangerEdge: () => invoke('take_arranger_edge'),
    // Show `path` (a log file or a bundle) in the system file manager.
    revealInFileManager: (path) => invoke('reveal_in_file_manager', { path }),
    // Collect every desk computer's logs into one bundle: {path, computers: [{id, name, included, reason?}]}.
    collectLogs: () => invoke('collect_logs'),
    // Answer an "Overwrite <name>?" question from a transfer notification.
    transferResolve: (transferId, overwrite) => invoke('transfer_resolve', { transferId, overwrite }),
    // Whether this run is portable (from a MouserData folder): nothing may be installed on this computer.
    portable: () => invoke('is_portable'),
    hideWindow: () => invoke('hide_window'),
    // macOS: stage this computer's keys and install the login-window agent, after one administrator prompt.
    allowLoginScreen: () => invoke('allow_login_screen'),
    // handler receives {method: topic, params}; resolves to an unlisten function.
    onNotification: (handler) => tauri.event.listen(EVENT, (event) => handler(event.payload)),
    // handler receives {x, y}: where the pointer landed, in the overlay window's own pixels.
    onLanding: (handler) => tauri.event.listen(LANDING_EVENT, (event) => handler(event.payload)),
    // handler receives {route}: the window should show that section (contracts/ui-navigation.md).
    onNavigate: (handler) => tauri.event.listen(NAVIGATE_EVENT, (event) => handler(event.payload)),
    // handler is told when QuickJump ends (the window is about to close).
    onQuickJump: (handler) => tauri.event.listen(QUICKJUMP_EVENT, () => handler()),
  };
}
