// Copyright (c) 2026 Faris Alfarhan
// SPDX-License-Identifier: GPL-3.0-only

const assert = require('node:assert');
const path = require('node:path');

const modulePath = process.argv[2];
if (!modulePath) {
  console.error('usage: node smoke.js <hal_wasm.js>');
  process.exit(2);
}

const wasm = require(path.resolve(modulePath));

function feed(text) {
  for (const byte of Buffer.from(text)) {
    wasm.feed_input_byte(byte);
  }
  wasm.tick();
}

function nonzero(buffer) {
  return buffer.reduce((count, byte) => count + (byte !== 0 ? 1 : 0), 0);
}

wasm.init();
assert.match(wasm.get_serial_output(), /> /);
assert.ok(nonzero(wasm.get_framebuffer()) > 0, 'startup display is blank');

feed('2+2\r');
assert.match(wasm.get_serial_output(), /= 4/);
assert.ok(nonzero(wasm.get_framebuffer()) > 0, 'scalar result display is blank');

wasm.init();
wasm.feed_toggle_mode();
wasm.feed_toggle_mode();
feed('[(1,2)(3,4)]\r');
assert.match(wasm.get_serial_output(), /2x2/);
assert.ok(nonzero(wasm.get_framebuffer()) > 0, 'matrix result display is blank');

wasm.init();
feed('2+2\r');
feed('3+3\r');
wasm.get_serial_output();
wasm.feed_cursor_key(0);
assert.match(wasm.get_serial_output(), /3\+3/);
wasm.feed_cursor_key(0);
assert.match(wasm.get_serial_output(), /2\+2/);
wasm.feed_cursor_key(1);
assert.match(wasm.get_serial_output(), /3\+3/);
wasm.feed_cursor_key(1);
const cleared = wasm.get_serial_output();
assert.match(cleared, /\r> /);
assert.ok(!cleared.includes('3+3'));

console.log('wasm smoke test passed');
