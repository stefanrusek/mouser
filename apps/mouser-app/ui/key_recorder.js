// Recording a hotkey by pressing it: a chord ("Ctrl+Alt+Shift+J") or the
// panic key and its press count ("Escape*2"). Key names are the ones the
// configuration accepts (config.rs `usage_for_name`).

const NAMED = ['Escape', 'Space', 'Enter', 'Tab', 'Backspace'];
const MAX_PRESSES = 5;

// The configuration's name for a KeyboardEvent `code`, or null when it has none.
export function keyName(code) {
  if (NAMED.includes(code)) return code;
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1];
  return /^F([1-9]|1[0-2])$/.test(code) ? code : null;
}

const MODIFIERS = [['ctrlKey', 'Ctrl'], ['altKey', 'Alt'], ['shiftKey', 'Shift'], ['metaKey', 'Meta']];

// Listen on `win` for the next key presses. mode 'chord': `onKey(accelerator)` once, then recording
// ends. mode 'panic': `onKey("<Key>*<count>")` on every press of a key (the count is how many times
// in a row it was pressed, 1 to 5) until `stop()`. Esc calls `onCancel()` and ends recording.
// Returns `stop`.
export function startRecording(win, mode, { onKey, onCancel }) {
  let last = null;
  let count = 0;
  const stop = () => win.removeEventListener('keydown', listener, true);
  function listener(event) {
    const key = keyName(event.code);
    if (key === null) return;
    event.preventDefault();
    if (key === 'Escape') {
      stop();
      onCancel();
    } else if (mode === 'chord') {
      stop();
      const held = MODIFIERS.filter(([flag]) => event[flag]).map(([, name]) => name);
      onKey([...held, key].join('+'));
    } else {
      count = key === last ? Math.min(count + 1, MAX_PRESSES) : 1;
      last = key;
      onKey(`${key}*${count}`);
    }
  }
  win.addEventListener('keydown', listener, true);
  return stop;
}
