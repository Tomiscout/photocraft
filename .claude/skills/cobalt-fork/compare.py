#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Renders effects through the Image as Audio app's CLI and PhotoCraft's `filter.audio.<effect>` commands
and checks that the RGB bytes are identical (cobalt fork, see SKILL.md).

    .claude/skills/cobalt-fork/compare.py [--photocraft-cli PATH] [--app PATH] [--image PATH]

Defaults: target/debug/photocraft-cli (build it first: cargo build -p photocraft-cli),
../image-as-audio/target/release/cobalt-native (cargo build --release there) and the app's
test/sample.avif. Exit status 1 when any effect differs or fails. Needs Pillow.
"""

import argparse
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[3]
APP_REPO = ROOT.parent / "image-as-audio"

# (name, app CLI effect spec, PhotoCraft params): every audio effect, plus each encoding,
# opacity and option kind at least once.
CASES = [
    ("wahwah", "wahwah", {}),
    ("reverb", "reverb:room-size=0.9", {"reverbRoomSize": 0.9}),
    ("compressor", "compressor:encoding=pcm8", {"encoding": "pcm8"}),
    ("limiter", "limiter", {}),
    ("amplify", "amplify", {}),
    ("paulstretch", "paulstretch", {}),
    ("parametriceq", "parametriceq:opacity=0.5", {"opacity": 0.5}),
    ("delay", "delay:delay-type=bouncingball,encoding=mulaw", {"delayDelayType": "bouncingball", "encoding": "mulaw"}),
    ("tremolo", "tremolo:waveform=square,sample-rate=8000", {"tremoloWaveform": "square", "sampleRate": 8000}),
    ("normalize", "normalize:remove-dc-offset=0", {"normalizeRemoveDcOffset": False}),
]


def rgb(path):
    return Image.open(path).convert("RGB").tobytes()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--photocraft-cli", default=str(ROOT / "target/debug/photocraft-cli"))
    ap.add_argument("--app", default=str(APP_REPO / "target/release/cobalt-native"))
    ap.add_argument("--image", default=str(APP_REPO / "test/sample.avif"))
    a = ap.parse_args()
    failed = False
    with tempfile.TemporaryDirectory() as d:
        src = f"{d}/in.png"
        # The app decodes the source; both sides then read the same 8-bit RGB PNG
        subprocess.run([a.app, "render", a.image, src], check=True, capture_output=True)
        for name, app_spec, params in CASES:
            subprocess.run([a.app, "render", src, f"{d}/app.png", "-e", app_spec], check=True, capture_output=True)
            t = time.time()
            r = subprocess.run(
                [a.photocraft_cli, "run", src, "--cmd", f"filter.audio.{name}", "--params", json.dumps(params), "--out", f"{d}/pc.png"],
                capture_output=True,
                text=True,
            )
            took = time.time() - t
            if r.returncode:
                print(f"{name:13} FAILED: {r.stderr.strip()[-400:]}")
                failed = True
                continue
            app, pc, orig = rgb(f"{d}/app.png"), rgb(f"{d}/pc.png"), rgb(src)
            differ = sum(x != y for x, y in zip(app, pc)) + abs(len(app) - len(pc))
            unchanged = app == orig
            ok = differ == 0 and not unchanged
            failed |= not ok
            note = "" if ok else ("  <- effect changed nothing" if unchanged else "  <- DIFFERS")
            print(f"{name:13} {differ} of {len(app)} bytes differ, photocraft-cli {took:.1f}s{note}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
