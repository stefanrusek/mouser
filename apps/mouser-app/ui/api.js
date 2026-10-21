// The UI's only way to reach the engine: Tauri commands and one event.

export const EVENT = 'mouser://notification';

export function makeApi(tauri) {
  const { invoke } = tauri.core;
  return {
    // Call a control-API method by name (contracts/control-api.md).
    call: (method, params = null) => invoke('control', { request: { method, params } }),
    openPermissionSettings: (kind) => invoke('open_permission_settings', { kind }),
    fix: (fix) => invoke('fix', { fix }),
    hideWindow: () => invoke('hide_window'),
    // handler receives {method: topic, params}; resolves to an unlisten function.
    onNotification: (handler) => tauri.event.listen(EVENT, (event) => handler(event.payload)),
  };
}
