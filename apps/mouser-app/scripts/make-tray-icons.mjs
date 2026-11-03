#!/usr/bin/env node
// Generates the tray icons: apps/mouser-app/icons/tray-<theme>-<state>-<connection>.png
//   theme:      light (dark glyph), dark (light glyph); macOS picks one by the menu bar's appearance
//   state:      active (filled mouse) | paused (outline mouse)
//   connection: connected (green filled disc) | partial (yellow filled triangle)
//               | disconnected (red hollow ring with a slash) | alone (no badge)
// The badges differ in shape as well as colour, so they stay apart in greyscale (SC-008).
// Usage: node scripts/make-tray-icons.mjs
import { deflateSync } from 'node:zlib';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const SIZE = 32;
const SUPER = 4;
const BADGE = { x: 24, y: 24 };

const GLYPH = { light: [28, 28, 28], dark: [242, 242, 242] };
const RED = [217, 45, 32];
const GREEN = [22, 163, 74];
const YELLOW = [234, 179, 8];
const BADGE_COLOUR = { connected: GREEN, partial: YELLOW, disconnected: RED };

const distanceToSegment = (px, py, ax, ay, bx, by) => {
  const dx = bx - ax;
  const dy = by - ay;
  const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)));
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
};
// A capsule standing at the middle of the icon: segment (16, 10.5)-(16, 21.5), radius r.
const capsule = (px, py, r) => distanceToSegment(px, py, 16, 10.5, 16, 21.5) <= r;

function glyph(px, py, state) {
  if (!capsule(px, py, 7.5)) return false;
  if (state === 'paused') return !capsule(px, py, 5);
  const divider = Math.abs(py - 14) < 0.8;
  const split = Math.abs(px - 16) < 0.8 && py < 14;
  return !(divider || split);
}

// Is (px, py) inside the triangle (ax, ay) (bx, by) (cx, cy)?
function inTriangle(px, py, [ax, ay], [bx, by], [cx, cy]) {
  const side = (x1, y1, x2, y2) => (x2 - x1) * (py - y1) - (y2 - y1) * (px - x1);
  const d1 = side(ax, ay, bx, by);
  const d2 = side(bx, by, cx, cy);
  const d3 = side(cx, cy, ax, ay);
  return !((d1 < 0 || d2 < 0 || d3 < 0) && (d1 > 0 || d2 > 0 || d3 > 0));
}

function badge(px, py, connection) {
  const d = Math.hypot(px - BADGE.x, py - BADGE.y);
  if (connection === 'connected') return d <= 4;
  if (connection === 'partial') return inTriangle(px, py, [BADGE.x, BADGE.y - 5.5], [BADGE.x + 5.5, BADGE.y + 4.5], [BADGE.x - 5.5, BADGE.y + 4.5]);
  if (connection === 'disconnected') {
    const ring = d <= 6 && d >= 3.6;
    const slash = distanceToSegment(px, py, BADGE.x - 4.2, BADGE.y + 4.2, BADGE.x + 4.2, BADGE.y - 4.2) <= 0.9 && d <= 6;
    return ring || slash;
  }
  return false;
}

const halo = (px, py, connection) => connection !== 'alone' && Math.hypot(px - BADGE.x, py - BADGE.y) <= 8;

function render(theme, state, connection) {
  const badgeColour = BADGE_COLOUR[connection];
  const rgba = Buffer.alloc(SIZE * SIZE * 4);
  for (let y = 0; y < SIZE; y += 1) {
    for (let x = 0; x < SIZE; x += 1) {
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;
      for (let sy = 0; sy < SUPER; sy += 1) {
        for (let sx = 0; sx < SUPER; sx += 1) {
          const px = x + (sx + 0.5) / SUPER;
          const py = y + (sy + 0.5) / SUPER;
          const colour = badge(px, py, connection)
            ? badgeColour
            : !halo(px, py, connection) && glyph(px, py, state)
              ? GLYPH[theme]
              : null;
          if (colour) {
            r += colour[0];
            g += colour[1];
            b += colour[2];
            a += 1;
          }
        }
      }
      const at = (y * SIZE + x) * 4;
      rgba[at] = a ? Math.round(r / a) : 0;
      rgba[at + 1] = a ? Math.round(g / a) : 0;
      rgba[at + 2] = a ? Math.round(b / a) : 0;
      rgba[at + 3] = Math.round((255 * a) / (SUPER * SUPER));
    }
  }
  return rgba;
}

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buffer) => {
  let c = 0xffffffff;
  for (const byte of buffer) c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const body = Buffer.concat([Buffer.from(type), data]);
  const out = Buffer.alloc(8 + data.length + 4);
  out.writeUInt32BE(data.length, 0);
  body.copy(out, 4);
  out.writeUInt32BE(crc32(body), 8 + data.length);
  return out;
};

export function png(rgba) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(SIZE, 0);
  header.writeUInt32BE(SIZE, 4);
  header.set([8, 6, 0, 0, 0], 8);
  const rows = Buffer.alloc((SIZE * 4 + 1) * SIZE);
  for (let y = 0; y < SIZE; y += 1) {
    rows[y * (SIZE * 4 + 1)] = 0;
    rgba.copy(rows, y * (SIZE * 4 + 1) + 1, y * SIZE * 4, (y + 1) * SIZE * 4);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', header),
    chunk('IDAT', deflateSync(rows, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

const icons = join(dirname(fileURLToPath(import.meta.url)), '..', 'icons');
mkdirSync(icons, { recursive: true });
for (const theme of ['light', 'dark']) {
  for (const state of ['active', 'paused']) {
    for (const connection of ['connected', 'partial', 'disconnected', 'alone']) {
      writeFileSync(join(icons, `tray-${theme}-${state}-${connection}.png`), png(render(theme, state, connection)));
    }
  }
}
