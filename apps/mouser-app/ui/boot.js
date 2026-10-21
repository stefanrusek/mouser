// Starts a window's module against the real Tauri globals.

import { makeApi } from './api.js';

export function boot(init, win = window) {
  return init({
    doc: win.document,
    api: makeApi(win.__TAURI__),
    every: (ms, tick) => win.setInterval(tick, ms),
    onFocus: (handler) => win.addEventListener('focus', handler),
  });
}
