// How numbers are shown to the owner (FR-009): rounded, never raw.

const UNITS = ['B', 'KB', 'MB', 'GB'];

// Whole milliseconds, or whole seconds from one second.
export function formatMs(ms) {
  return ms < 999.5 ? `${Math.round(ms)} ms` : `${Math.round(ms / 1000)} s`;
}

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
  const whole = Math.round(seconds);
  return whole < 60 ? `${whole} s left` : `${Math.floor(whole / 60)} min ${whole % 60} s left`;
}

export const formatScale = (scale) => `${Math.round(scale * 100)}%`;
