#!/usr/bin/env python3
"""Package manifested native binaries and embedded-asset documentation, without user data."""

import argparse
import gzip
import io
import json
import os
from pathlib import Path
import tarfile
import zipfile
from common import ROOT, TARGETS, publish, revision, sha, validate_binary, version


def assemble(target, binary, output, native=None):
    if target not in TARGETS:
        raise ValueError("Unsupported target: " + target)
    files = {}

    def add(name, path, executable=False):
        if path.is_symlink() or not path.is_file():
            raise ValueError("Missing or symlinked package input: " + str(path))
        files[name] = (path.read_bytes(), executable)

    linux = "linux" in target
    windows = "windows" in target
    add(
        "bin/meshcraft" if linux else "meshcraft.exe" if windows else "meshcraft",
        binary,
        not windows,
    )
    if linux:
        if native is None:
            raise ValueError("Linux packages require an audited native bundle")
        report = json.loads((native / "native-manifest.json").read_text())
        if report["target"] != target:
            raise ValueError("Native bundle target mismatch")
        for path in sorted(native.rglob("*")):
            if path.is_file():
                add(
                    path.relative_to(native).as_posix(),
                    path,
                    bool(path.stat().st_mode & 0o111),
                )
        # Native bundle includes the patched executable. Do not overwrite it with the original.
        add("meshcraft", ROOT / "packaging/linux/meshcraft", True)
    for name in ["README.md", "LICENSE-MIT", "LICENSE-APACHE", "packaging/README.md"]:
        add(name, ROOT / name)
    add("notices/tool-icons.txt", ROOT / "assets/tool-icons/LEGACY-LICENSE.md")
    add("meshcraft.svg", ROOT / "packaging/meshcraft.svg")
    add("meshcraft.png", ROOT / "packaging/meshcraft.png")
    add("examples/dynamic-object.octd", ROOT / "examples/dynamic-object.octd")
    executable = (
        "bin/meshcraft" if linux else "meshcraft.exe" if windows else "meshcraft"
    )
    validate_binary(target, files[executable][0])
    app_version = version()
    manifest = dict(
        schemaVersion=1,
        target=target,
        version=app_version,
        tag=app_version,
        sourceRevision=revision(),
        assets="embedded in executable",
        files={
            name: dict(sha256=sha(data), size=len(data), executable=x)
            for name, (data, x) in sorted(files.items())
        },
    )
    if linux:
        manifest["minimumGlibc"] = report["minimumGlibc"]
    files["package-manifest.json"] = (
        (json.dumps(manifest, sort_keys=True, indent=2) + "\n").encode(),
        False,
    )
    name = "meshcraft-" + app_version + "-" + target
    output.mkdir(parents=True, exist_ok=True)
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "1767225600"))
    archive = output / (name + ".tar.gz")
    with archive.open("wb") as raw, gzip.GzipFile(
        filename="", fileobj=raw, mode="wb", mtime=epoch
    ) as compressed:
        with tarfile.open(
            fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT
        ) as tar:
            for relative, (data, x) in sorted(files.items()):
                info = tarfile.TarInfo(name + "/" + relative)
                info.size = len(data)
                info.mtime = epoch
                info.mode = 0o755 if x else 0o644
                tar.addfile(info, io.BytesIO(data))
    zip_path = output / (name + ".zip")
    with zipfile.ZipFile(
        zip_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as zipped:
        for relative, (data, x) in sorted(files.items()):
            info = zipfile.ZipInfo(
                name + "/" + relative, date_time=(2026, 1, 1, 0, 0, 0)
            )
            info.external_attr = (0o100755 if x else 0o100644) << 16
            zipped.writestr(
                info, data, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9
            )
    for path in [archive, zip_path]:
        publish(path)
    return archive


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=ROOT / "target/release-assets")
    parser.add_argument("--native", type=Path)
    a = parser.parse_args()
    assemble(a.target, a.binary, a.output, a.native)
