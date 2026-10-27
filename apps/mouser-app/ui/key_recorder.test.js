import assert from 'node:assert/strict';
import test from 'node:test';
import { Window } from 'happy-dom';
import { keyName, startRecording } from './key_recorder.js';

const press = (win, init) => {
  const event = new win.KeyboardEvent('keydown', { cancelable: true, bubbles: true, ...init });
  win.dispatchEvent(event);
  return event;
};

function record(mode) {
  const win = new Window({ url: 'http://localhost/index.html' });
  const log = { keys: [], cancels: 0 };
  const stop = startRecording(win, mode, { onKey: (k) => log.keys.push(k), onCancel: () => log.cancels++ });
  return { win, log, stop };
}

test('key names are the ones the configuration accepts', () => {
  assert.equal(keyName('KeyM'), 'M');
  assert.equal(keyName('Digit5'), '5');
  assert.equal(keyName('F12'), 'F12');
  assert.equal(keyName('F1'), 'F1');
  for (const name of ['Escape', 'Space', 'Enter', 'Tab', 'Backspace']) assert.equal(keyName(name), name);
  assert.equal(keyName('ArrowUp'), null);
  assert.equal(keyName('F13'), null);
});

test('recording a chord captures the pressed modifiers and key as an accelerator string', () => {
  const { win, log } = record('chord');
  press(win, { code: 'ControlLeft', ctrlKey: true });
  assert.deepEqual(log.keys, [], 'a modifier alone is not a chord');
  const event = press(win, { code: 'KeyJ', ctrlKey: true, altKey: true, shiftKey: true });
  assert.deepEqual(log.keys, ['Ctrl+Alt+Shift+J']);
  assert.equal(event.defaultPrevented, true);
  press(win, { code: 'KeyK', ctrlKey: true });
  assert.deepEqual(log.keys, ['Ctrl+Alt+Shift+J'], 'recording ended with the chord');
});

test('a chord can use Meta, and a key the configuration cannot name is ignored', () => {
  const { win, log } = record('chord');
  press(win, { code: 'ArrowUp' });
  press(win, { code: 'KeyV', metaKey: true });
  assert.deepEqual(log.keys, ['Meta+V']);
});

test('recording the panic key captures one key plus a press count from 1 to 5', () => {
  const { win, log } = record('panic');
  press(win, { code: 'KeyP', ctrlKey: true });
  press(win, { code: 'KeyP' });
  press(win, { code: 'KeyP' });
  press(win, { code: 'F9' });
  for (let i = 0; i < 6; i++) press(win, { code: 'F9' });
  assert.deepEqual(log.keys, ['P*1', 'P*2', 'P*3', 'F9*1', 'F9*2', 'F9*3', 'F9*4', 'F9*5', 'F9*5', 'F9*5']);
});

test('the panic recorder ignores a modifier alone and a key it cannot name', () => {
  const { win, log } = record('panic');
  press(win, { code: 'ShiftLeft', shiftKey: true });
  press(win, { code: 'ArrowUp' });
  assert.deepEqual(log.keys, []);
});

test('Esc cancels recording', () => {
  for (const mode of ['chord', 'panic']) {
    const { win, log } = record(mode);
    press(win, { code: 'Escape' });
    assert.equal(log.cancels, 1);
    assert.deepEqual(log.keys, []);
    press(win, { code: 'KeyA' });
    assert.deepEqual(log.keys, [], 'recording ended');
  }
});

test('stopping ends recording', () => {
  const { win, log, stop } = record('panic');
  stop();
  press(win, { code: 'KeyA' });
  assert.deepEqual(log.keys, []);
});
