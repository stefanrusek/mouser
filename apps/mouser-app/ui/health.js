// The health panel: each computer's connection, speed, missing permissions,
// limitations and latest problem, plus diagnostic logging (Story 8).

import { button, el, guarded, show } from './dom.js';

const CONNECTION_TEXT = {
  self: 'This computer',
  connected: 'Connected',
  reconnecting: 'Reconnecting',
  offline: 'Offline',
  incompatible: 'Needs an update',
};

const PERMISSION_TEXT = {
  accessibility: 'Accessibility',
  input_monitoring: 'Input Monitoring',
  screen_recording: 'Screen Recording',
  full_disk_access: 'Full Disk Access',
  input_capture_portal: 'Input capture',
  remote_desktop_portal: 'Remote desktop',
};

export const permissionLabel = (kind) => PERMISSION_TEXT[kind];

export function responsiveness(rttMs) {
  if (rttMs <= 30) return `Responds quickly (${rttMs} ms)`;
  return rttMs <= 100 ? `Responds a little slowly (${rttMs} ms)` : `Responds slowly (${rttMs} ms)`;
}

// The open problem of `computerId` that was raised last.
export function latestProblem(problems, computerId) {
  return problems
    .filter((p) => p.computer === computerId && !p.cleared_at)
    .sort((a, b) => b.raised_at.localeCompare(a.raised_at))[0];
}

function renderComputer(doc, computer, problems, selfId, onFix) {
  const mine = computer.id === selfId;
  const item = el(
    doc,
    'li',
    {},
    el(doc, 'span', { class: 'what' },
      el(doc, 'strong', { class: 'name' }, mine ? `${computer.name} (this computer)` : computer.name),
      el(doc, 'small', { class: 'connection' }, CONNECTION_TEXT[computer.connection]),
    ),
  );
  const what = item.querySelector('.what');
  if (computer.rtt_ms != null) what.append(el(doc, 'small', { class: 'rtt' }, responsiveness(computer.rtt_ms)));
  for (const [kind, state] of Object.entries(computer.permissions)) {
    if (state !== 'missing') continue;
    const line = el(doc, 'small', { class: 'permission' }, `${permissionLabel(kind)} is missing. `);
    const open = mine && problems.find((p) => p.computer === selfId && !p.cleared_at && p.fix === `open_system_settings:${kind}`);
    if (open) line.append(button(doc, 'Open settings', () => onFix(open)));
    what.append(line);
  }
  for (const limitation of computer.limitations) what.append(el(doc, 'small', { class: 'limitation' }, limitation));
  const latest = latestProblem(problems, computer.id);
  if (latest) what.append(el(doc, 'small', { class: 'problem' }, latest.message));
  return item;
}

export async function init({ doc, api, every, onFocus }) {
  const errorLine = doc.getElementById('error');
  const logging = doc.getElementById('logging');
  const openLog = doc.getElementById('open-log');
  const state = { logPath: '' };

  const fix = guarded(errorLine, (problem) => api.fix(problem.problem_id, problem.fix));
  const showLog = (enabled) => show(openLog, enabled);

  async function refresh() {
    const status = await api.call('status');
    doc.getElementById('computers').replaceChildren(
      ...status.computers.map((c) => renderComputer(doc, c, status.problems, status.computer.id, fix)),
    );
    logging.checked = status.logging.enabled;
    state.logPath = status.logging.path;
    showLog(status.logging.enabled);
  }
  const tick = guarded(errorLine, refresh, { clear: false });

  logging.addEventListener(
    'change',
    guarded(errorLine, async () => {
      const { path } = await api.call('logging.set', { enabled: logging.checked });
      state.logPath = path;
      showLog(logging.checked);
    }),
  );
  openLog.addEventListener('click', guarded(errorLine, () => api.openFolder(state.logPath)));

  await api.onNotification((note) => {
    if (note.method === 'status' || note.method === 'problem') tick();
  });
  every(3000, tick);
  onFocus(tick);
  await guarded(errorLine, refresh)();
  return { tick };
}
