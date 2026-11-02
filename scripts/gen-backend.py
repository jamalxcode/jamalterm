#!/usr/bin/env python3
"""Rewrites the method list in frontend/src/backend.ts from the generated
bindings (run by `make bindings`; CI checks it's up to date)."""
import re, pathlib
root = pathlib.Path(__file__).resolve().parent.parent
names = re.findall(r'^export function (\w+)\(', (root / 'frontend/bindings/jamalterm/app.ts').read_text(), re.M)
body = '\n'.join('  ' + ', '.join(names[i:i + 6]) + ',' for i in range(0, len(names), 6))
p = root / 'frontend/src/backend.ts'
s = p.read_text()
s = re.sub(r'export const \{\n.*?\n\} = App', 'export const {\n' + body + '\n} = App', s, flags=re.S)
p.write_text(s)
