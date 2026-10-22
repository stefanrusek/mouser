// Shared by the UI tests: load a page into happy-dom and fake the engine.

import { readFileSync } from 'node:fs';
import { Window } from 'happy-dom';

export function loadPage(name) {
  const html = readFileSync(new URL(`./${name}`, import.meta.url), 'utf8');
  const win = new Window({
    settings: { disableJavaScriptEvaluation: true, disableJavaScriptFileLoading: true },
  });
  win.document.documentElement.innerHTML = html.replace(/^<!doctype html>/i, '');
  return win;
}

export const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

// A fake api: `answers` maps method -> value or function(params); every call is recorded.
export function fakeApi(answers = {}) {
  const calls = [];
  const listeners = [];
  const landings = [];
  const quickJumps = [];
  const record = (name, args) => calls.push([name, ...args]);
  return {
    calls,
    emitLanding: (landing) => landings.forEach((handler) => handler(landing)),
    emitQuickJump: () => quickJumps.forEach((handler) => handler()),
    emit: (method, params) => listeners.forEach((handler) => handler({ jsonrpc: '2.0', method, params })),
    call: async (method, params) => {
      calls.push([method, params]);
      const answer = answers[method];
      const value = typeof answer === 'function' ? answer(params) : answer;
      if (value instanceof Error) throw value;
      return value;
    },
    openPermissionSettings: async (kind) => record('open_permission_settings', [kind]),
    fix: async (problemId, fix) => {
      record('fix', [problemId, fix]);
      if (answers.fixError) throw answers.fixError;
    },
    transferResolve: async (transferId, overwrite) => {
      record('transfer_resolve', [transferId, overwrite]);
      if (answers.transfer_resolve instanceof Error) throw answers.transfer_resolve;
    },
    transfers: async () => answers.transfers ?? [],
    takeArrangerEdge: async () => answers.arrangerEdge ?? null,
    openFolder: async (path) => record('open_folder', [path]),
    hideWindow: async () => record('hide_window', []),
    allowLoginScreen: async () => {
      record('allow_login_screen', []);
      if (answers.allow_login_screen instanceof Error) throw answers.allow_login_screen;
    },
    onLanding: async (handler) => {
      landings.push(handler);
    },
    onQuickJump: async (handler) => {
      quickJumps.push(handler);
    },
    onNotification: async (handler) => {
      listeners.push(handler);
    },
  };
}

export function click(win, node) {
  node.dispatchEvent(new win.MouseEvent('click', { bubbles: true }));
}
