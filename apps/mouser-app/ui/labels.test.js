import assert from 'node:assert/strict';
import test from 'node:test';
import { CONNECTION_TEXT, OS_NAMES, PERMISSIONS, permissionLabel } from './labels.js';

test('every connection state has one text', () => {
  assert.deepEqual(CONNECTION_TEXT, {
    self: 'This computer',
    connected: 'Connected',
    reconnecting: 'Reconnecting',
    offline: 'Offline',
    incompatible: 'Needs an update',
  });
});

test('every permission has a label and a reason', () => {
  assert.deepEqual(Object.keys(PERMISSIONS), [
    'accessibility',
    'input_monitoring',
    'screen_recording',
    'full_disk_access',
    'input_capture_portal',
    'remote_desktop_portal',
  ]);
  for (const { title, why } of Object.values(PERMISSIONS)) {
    assert.ok(title && why);
  }
  assert.equal(permissionLabel('input_monitoring'), 'Input Monitoring');
});

test('operating systems have display names', () => {
  assert.deepEqual(OS_NAMES, { macos: 'macOS', windows: 'Windows', linux: 'Linux' });
});
