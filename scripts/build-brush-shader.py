#!/usr/bin/env python3
"""Compile the bundled compute shaders with the SDK's glslangValidator; --check detects stale bundled words."""
import argparse
from pathlib import Path
import re
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
# (source, generated header, C array name)
shaders = [
    ('backends/brush/shaders/continuous_brush.comp', 'backends/brush/shaders/continuous_brush_spv.h', 'compositor_brush_spv'),
    ('backends/effects/shaders/effects.comp', 'backends/effects/shaders/effects_spv.h', 'compositor_effects_spv'),
]
words = lambda text: re.findall(r'0x[0-9a-fA-F]+', text)
for source_path, header_path, name in shaders:
    source, header = root / source_path, root / header_path
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'shader.h'
        subprocess.run(['glslangValidator', '-V', '--target-env', 'vulkan1.0', '--vn', name, '-o', str(output), str(source)], check=True)
        generated = output.read_text()
        if args.check:
            if words(generated) != words(header.read_text()):
                raise SystemExit(f'Bundled SPIR-V for {source_path} is stale; regenerate inside the SDK.')
        else:
            header.write_text(generated)
