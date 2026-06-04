let wasm = null;
const canvas = document.getElementById('display');
const ctx = canvas.getContext('2d');
const term = document.getElementById('term');
const modeIndicator = document.getElementById('modeIndicator');
const SCALE = 4;
const W = 96;
const H = 16;

canvas.width = W * SCALE;
canvas.height = H * SCALE;

const MODE_NAMES = ['Standard', 'Advanced', 'Matrix', 'Scientific'];

// Terminal emulator: maintains a line buffer that respects backspace
let termLines = [''];

function processSerialOutput(text) {
  for (const ch of text) {
    if (ch === '\r') {
      // ignore, \n handles newlines
    } else if (ch === '\n') {
      termLines.push('');
    } else if (ch === '\x08') {
      // backspace: remove last char from current line
      if (termLines.length > 0 && termLines[termLines.length - 1].length > 0) {
        termLines[termLines.length - 1] = termLines[termLines.length - 1].slice(0, -1);
      }
    } else {
      if (termLines.length === 0) termLines.push('');
      termLines[termLines.length - 1] += ch;
    }
  }
  // Keep only last 200 lines
  if (termLines.length > 200) {
    termLines = termLines.slice(termLines.length - 200);
  }
  term.textContent = termLines.join('\n');
  term.scrollTop = term.scrollHeight;
}

function renderFrame() {
  if (!wasm) return;
  const buf = wasm.get_framebuffer();
  const cw = canvas.width;
  const imageData = ctx.createImageData(cw, canvas.height);
  const data = imageData.data;

  for (let col = 0; col < W; col++) {
    for (let page = 0; page < 2; page++) {
      const byte = buf[page * W + col];
      for (let bit = 0; bit < 8; bit++) {
        const row = page * 8 + bit;
        const on = (byte >> bit) & 1;
        const r = on ? 0x0f : 0x9b;
        const g = on ? 0x38 : 0xbc;
        const b = on ? 0x0f : 0x0f;
        for (let dy = 0; dy < SCALE; dy++) {
          for (let dx = 0; dx < SCALE; dx++) {
            const px = col * SCALE + dx;
            const py = row * SCALE + dy;
            const idx = (py * cw + px) * 4;
            data[idx] = r;
            data[idx + 1] = g;
            data[idx + 2] = b;
            data[idx + 3] = 255;
          }
        }
      }
    }
  }

  ctx.putImageData(imageData, 0, 0);
}

function updateSerial() {
  if (!wasm) return;
  const out = wasm.get_serial_output();
  if (out) {
    processSerialOutput(out);
  }
}

function updateMode() {
  if (!wasm) return;
  const modeByte = wasm.get_mode();
  modeIndicator.textContent = MODE_NAMES[modeByte] + ' mode';
}

function feedAll(key, e) {
  if (!wasm) return;
  e.preventDefault();

  if (key === 'Enter') {
    wasm.feed_input_byte(0x0d);
  } else if (key === 'Backspace') {
    wasm.feed_input_byte(0x08);
  } else if (key === 'Escape') {
    wasm.feed_toggle_mode();
  } else if (key.startsWith('Arrow')) {
    const dirs = { ArrowUp: 0, ArrowDown: 1, ArrowLeft: 2, ArrowRight: 3 };
    wasm.feed_cursor_key(dirs[key]);
  } else if (e.ctrlKey && (key === 'd' || key === 'D')) {
    wasm.feed_toggle_angle();
  } else if (key.length === 1) {
    const code = key.charCodeAt(0);
    if (code >= 0x20 && code <= 0x7e) {
      wasm.feed_input_byte(code);
    }
  }

  wasm.tick();
  renderFrame();
  updateSerial();
  updateMode();
}

document.addEventListener('keydown', (e) => {
  if (!wasm) return;
  const key = e.key;
  if (key === 'Enter' || key === 'Backspace' || key === 'Escape' ||
      key.startsWith('Arrow') || (e.ctrlKey && (key === 'd' || key === 'D')) ||
      key.length === 1) {
    feedAll(key, e);
  }
});

document.querySelectorAll('.controls button').forEach(btn => {
  btn.addEventListener('click', () => {
    const keyMap = {
      esc: 'Escape', backspace: 'Backspace', enter: 'Enter',
      up: 'ArrowUp', down: 'ArrowDown', left: 'ArrowLeft', right: 'ArrowRight'
    };
    const mapped = keyMap[btn.dataset.key];
    if (mapped) feedAll(mapped, { preventDefault: () => {} });
  });
});

wasm_bindgen('./pkg/hal_wasm_bg.wasm').then(() => {
  wasm = wasm_bindgen;
  wasm.init();
  renderFrame();
  updateSerial();
  updateMode();
  setInterval(renderFrame, 100);
}).catch(console.error);
