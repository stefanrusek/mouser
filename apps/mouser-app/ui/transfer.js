// The file-transfer window: progress, "Overwrite <name>?" and "not enough space" (Story 7).

import { button, el, guarded } from './dom.js';

const UNITS = ['B', 'KB', 'MB', 'GB'];

export function formatBytes(bytes) {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${value} B` : `${value.toFixed(1)} ${UNITS[unit]}`;
}

export const formatRate = (rate) => `${formatBytes(rate)}/s`;

export function formatEta(seconds) {
  return seconds < 60 ? `${seconds} s left` : `${Math.floor(seconds / 60)} min ${seconds % 60} s left`;
}

const STATE_LABELS = { pending: 'Waiting', checking: 'Waiting', copying: 'Copying', completed: 'Done' };

export function renderTransfer(doc, transfer, onResolve) {
  const card = el(doc, 'div', { class: 'card' });
  if (transfer.state === 'rejected') {
    const text =
      transfer.reason === 'no_space'
        ? 'There is not enough space on the other computer. Nothing was copied.'
        : transfer.message;
    card.append(el(doc, 'p', { class: 'rejected' }, text));
    return card;
  }
  if (transfer.state === 'failed') {
    card.append(el(doc, 'p', { class: 'failed' }, transfer.message));
    return card;
  }
  card.append(el(doc, 'h2', { class: 'item' }, transfer.current_item || 'Getting ready'));
  if (transfer.state === 'awaiting_overwrite_decision') {
    card.append(
      el(doc, 'p', { class: 'question' }, `Overwrite ${transfer.item}?`),
      el(
        doc,
        'div',
        { class: 'row' },
        button(doc, 'Overwrite', () => onResolve(transfer, true), { class: 'primary' }),
        button(doc, 'Skip', () => onResolve(transfer, false)),
      ),
    );
    return card;
  }
  card.append(el(doc, 'p', { class: 'state hint' }, STATE_LABELS[transfer.state]));
  if (transfer.bytes_total > 0) {
    card.append(
      el(doc, 'progress', { value: transfer.bytes_done, max: transfer.bytes_total }),
      el(doc, 'p', { class: 'amount' }, `${formatBytes(transfer.bytes_done)} of ${formatBytes(transfer.bytes_total)}`),
    );
  }
  if (transfer.rate > 0) card.append(el(doc, 'p', { class: 'speed hint' }, formatRate(transfer.rate)));
  if (transfer.eta_seconds != null) card.append(el(doc, 'p', { class: 'eta hint' }, formatEta(transfer.eta_seconds)));
  return card;
}

export async function init({ doc, api }) {
  const container = doc.getElementById('transfers');
  const errorLine = doc.getElementById('error');
  let transfers = [];

  const draw = () => container.replaceChildren(...transfers.map((t) => renderTransfer(doc, t, resolve)));
  const resolve = guarded(errorLine, (transfer, overwrite) =>
    api.transferResolve(transfer.transfer_id, overwrite),
  );

  const upsert = (next) => {
    const known = transfers.some((t) => t.transfer_id === next.transfer_id);
    transfers = known ? transfers.map((t) => (t.transfer_id === next.transfer_id ? next : t)) : [...transfers, next];
  };
  const live = new Set();
  await api.onNotification((note) => {
    if (note.method !== 'transfer') return;
    live.add(note.params.transfer_id);
    upsert(note.params);
    draw();
  });
  // The window opens after the notification that opened it: take what the app kept, unless a newer one came.
  for (const saved of await api.transfers()) {
    if (!live.has(saved.transfer_id)) upsert(saved);
  }
  draw();
  return { transfers: () => transfers };
}
