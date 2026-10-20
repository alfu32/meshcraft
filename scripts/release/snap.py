#!/usr/bin/env python3
"""Pack a desktop Snap including its audited native dependencies."""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from common import ROOT, payload, publish, sha


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--archive", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    a.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="meshcraft-snap-") as temp:
        stage = Path(temp)
        package, manifest = payload(a.archive, stage / "input")
        if "linux" not in manifest["target"]:
            raise ValueError("Snap needs a Linux payload")
        arch = "arm64" if manifest["target"].startswith("aarch64") else "amd64"
        root = stage / "snap"
        (root / "opt").mkdir(parents=True)
        shutil.copytree(package, root / "opt/meshcraft")
        (root / "bin").mkdir()
        (root / "gpu-2404").mkdir()
        (root / "bin/meshcraft").write_text(
            '#!/bin/sh\nexport XDG_CONFIG_HOME="$SNAP_USER_COMMON/.config"\nexport XDG_CACHE_HOME="$SNAP_USER_COMMON/.cache"\nexec "$SNAP/gpu-2404/bin/gpu-2404-provider-wrapper" "$SNAP/opt/meshcraft/meshcraft" "$@"\n'
        )
        (root / "bin/meshcraft").chmod(0o755)
        (root / "meta/gui").mkdir(parents=True)
        shutil.copyfile(
            ROOT / "packaging/meshcraft.svg", root / "meta/gui/meshcraft.svg"
        )
        desktop = (
            (ROOT / "packaging/linux/meshcraft.desktop")
            .read_text()
            .replace("Icon=meshcraft", "Icon=${SNAP}/meta/gui/meshcraft.svg")
        )
        (root / "meta/gui/meshcraft.desktop").write_text(desktop)
        (root / "meta/snap.yaml").write_text(
            """name: meshcraft
version: '{version}'
summary: Native 3D construction and modeling
description: |
  Meshcraft GPUI desktop with an offscreen raylib viewport.
base: core24
architectures: [{arch}]
plugs:
  gpu-2404:
    interface: content
    content: gpu-2404
    default-provider: mesa-2404
    target: $SNAP/gpu-2404
confinement: strict
grade: devel
license: MIT OR Apache-2.0
apps:
  meshcraft:
    command: bin/meshcraft
    plugs: [home, removable-media, desktop, desktop-legacy, x11, opengl, network, gpu-2404]
""".format(version=manifest["version"], arch=arch)
        )
        epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "1767225600"))
        for path in sorted(root.rglob("*"), reverse=True):
            os.utime(path, (epoch, epoch))
        subprocess.run(
            ["snap", "pack", str(root), str(a.output.resolve()), "--compression=xz"],
            check=True,
        )
        artifact = a.output / (
            "meshcraft_" + manifest["version"] + "_" + arch + ".snap"
        )
        if not artifact.is_file():
            raise ValueError("Snap artifact missing")
        publish(
            artifact,
            dict(
                format="snap",
                target=manifest["target"],
                base="core24",
                confinement="strict",
                minimumGlibc=manifest["minimumGlibc"],
                payloadArchiveSha256=sha(a.archive.read_bytes()),
            ),
        )


if __name__ == "__main__":
    main()
