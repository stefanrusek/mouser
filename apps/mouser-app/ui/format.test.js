import assert from 'node:assert/strict';
import test from 'node:test';
import { formatBytes, formatEta, formatMs, formatRate, formatScale } from './format.js';

test('durations are whole milliseconds, or whole seconds from one second', () => {
  assert.equal(formatMs(8.707351000000001), '9 ms');
  assert.equal(formatMs(0.2), '0 ms');
  assert.equal(formatMs(999.4), '999 ms');
  assert.equal(formatMs(1500), '2 s');
  assert.equal(formatMs(12000.7), '12 s');
});

test('sizes are whole bytes or one decimal in larger units', () => {
  assert.equal(formatBytes(0), '0 B');
  assert.equal(formatBytes(512), '512 B');
  assert.equal(formatBytes(1536), '1.5 KB');
  assert.equal(formatBytes(5 * 1024 * 1024), '5.0 MB');
  assert.equal(formatBytes(3 * 1024 ** 3), '3.0 GB');
  assert.equal(formatBytes(2048 * 1024 ** 3), '2048.0 GB');
});

test('a rate is a size per second and an estimate is rounded to whole seconds', () => {
  assert.equal(formatRate(1536), '1.5 KB/s');
  assert.equal(formatEta(42), '42 s left');
  assert.equal(formatEta(41.6), '42 s left');
  assert.equal(formatEta(125), '2 min 5 s left');
});

test('a display scale is a percentage', () => {
  assert.equal(formatScale(2), '200%');
  assert.equal(formatScale(1.5), '150%');
  assert.equal(formatScale(1.7499999), '175%');
});
