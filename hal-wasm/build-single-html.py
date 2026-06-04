#!/usr/bin/env python3
"""Build a single self-contained offline HTML file for the NumCore simulator."""
import base64, os, sys

PUB = os.path.join(os.path.dirname(__file__), 'www')
PKG = os.path.join(PUB, 'pkg')

# Read source files
with open(os.path.join(PUB, 'index.html'), 'r') as f:
    html = f.read()

with open(os.path.join(PKG, 'hal_wasm.js'), 'r') as f:
    wasm_js = f.read()

with open(os.path.join(PUB, 'index.js'), 'r') as f:
    index_js = f.read()

with open(os.path.join(PKG, 'hal_wasm_bg.wasm'), 'rb') as f:
    wasm_b64 = base64.b64encode(f.read()).decode('ascii')

# Remove external script tags
html = html.replace('<script src="pkg/hal_wasm.js"></script>', '')
html = html.replace('<script src="index.js"></script>', '')

# Inject inline scripts
inline_scripts = f'''
<script>
// — WASM binary (base64-encoded, 317 KB raw / 431 KB text)
const WASM_BASE64 = `{wasm_b64}`;

// — wasm-bindgen runtime (auto-generated)
{wasm_js}

// — Simulator entry point
{index_js}
</script>
'''

# Insert before </body> or at end
if '</body>' in html:
    html = html.replace('</body>', inline_scripts + '\n</body>')
else:
    html += inline_scripts

# Override JS init to use initSync
html = html.replace(
    "wasm_bindgen('./pkg/hal_wasm_bg.wasm').then(() => {",
    '''// Synchronous init from embedded base64
const wasmBinary = Uint8Array.from(atob(WASM_BASE64), c => c.charCodeAt(0));
wasm_bindgen.initSync(wasmBinary);
(function() {'''
)

# Close the IIFE
html = html.replace('}).catch(console.error);', '})();')

out_path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(PUB, 'index-standalone.html')
os.makedirs(os.path.dirname(out_path) or '.', exist_ok=True)
with open(out_path, 'w') as f:
    f.write(html)

size_kb = os.path.getsize(out_path) / 1024
print(f'Wrote {out_path}  ({size_kb:.0f} KB)')
