#!/usr/bin/env python3
"""Require every native target, installer, digest and matching portable payload before publication."""

import argparse
import io
import json
from pathlib import Path
import tarfile
import zipfile
from common import ROOT, TARGETS, safe, sha, validate_binary


def expected_assets(version):
    names = set()
    for target, installers in TARGETS.items():
        base = "meshcraft-" + version + "-" + target
        names.update([base + ".zip", base + ".tar.gz"])
        for suffix in installers:
            names.add(
                "meshcraft_" + version + "_" + suffix
                if suffix.endswith(".snap")
                else "meshcraft-" + version + "-" + suffix
            )
    return names


def check_portable(data, name, version, revision):
    root = name[:-7] if name.endswith(".tar.gz") else name[:-4]
    target = root[len("meshcraft-" + version + "-") :]
    contents = {}
    if name.endswith(".tar.gz"):
        with tarfile.open(fileobj=io.BytesIO(data)) as a:
            for m in a.getmembers():
                safe(m.name)
                if not m.isfile() or m.name in contents:
                    raise ValueError("Invalid archive member")
                contents[m.name] = a.extractfile(m).read()
    else:
        with zipfile.ZipFile(io.BytesIO(data)) as a:
            for m in a.infolist():
                safe(m.filename)
                if m.is_dir() or m.filename in contents:
                    raise ValueError("Invalid ZIP member")
                contents[m.filename] = a.read(m)
    manifest = json.loads(contents[root + "/package-manifest.json"])
    if (
        manifest["version"],
        manifest["tag"],
        manifest["target"],
        manifest["sourceRevision"],
    ) != (version, version, target, revision):
        raise ValueError("Payload release source mismatch")
    if set(contents) != {root + "/" + p for p in manifest["files"]} | {
        root + "/package-manifest.json"
    }:
        raise ValueError("Portable inventory mismatch")
    executable = (
        "bin/meshcraft"
        if "linux" in target
        else "meshcraft.exe"
        if "windows" in target
        else "meshcraft"
    )
    if executable not in manifest["files"]:
        raise ValueError("Application binary missing")
    validate_binary(target, contents[root + "/" + executable])
    for p, entry in manifest["files"].items():
        safe(p)
        value = contents[root + "/" + p]
        if sha(value) != entry["sha256"] or len(value) != entry["size"]:
            raise ValueError("Portable file digest mismatch: " + p)
    if "linux" in target and tuple(map(int, manifest["minimumGlibc"].split("."))) > (
        2,
        31,
    ):
        raise ValueError("Linux payload exceeds glibc ceiling")


def verify(directory, version, revision):
    expected = expected_assets(version)
    actual = {
        p.name
        for p in directory.iterdir()
        if p.is_file() and not p.name.endswith((".sha256", ".manifest.json"))
    }
    if actual != expected:
        raise ValueError(
            "Release matrix differs: missing="
            + str(sorted(expected - actual))
            + " unexpected="
            + str(sorted(actual - expected))
        )
    checksums = []
    for name in sorted(expected):
        data = (directory / name).read_bytes()
        if not data:
            raise ValueError("Empty release artifact: " + name)
        digest = sha(data)
        if (directory / (name + ".sha256")).read_text().strip() != digest + "  " + name:
            raise ValueError("Invalid checksum: " + name)
        checksums.append(digest + "  " + name)
        if name.endswith((".zip", ".tar.gz")):
            check_portable(data, name, version, revision)
        elif name.endswith(".msix"):
            with zipfile.ZipFile(io.BytesIO(data)) as a:
                if not {"AppxManifest.xml", "AppxBlockMap.xml"}.issubset(a.namelist()):
                    raise ValueError("Invalid MSIX")
        elif name.endswith(".msi") and not data.startswith(
            bytes.fromhex("d0cf11e0a1b11ae1")
        ):
            raise ValueError("Invalid MSI")
        elif name.endswith(".AppImage") and (
            data[:4] != b"\x7fELF" or data[8:11] != b"AI\x02"
        ):
            raise ValueError("Invalid AppImage")
        elif name.endswith(".snap") and data[:4] != b"hsqs":
            raise ValueError("Invalid Snap")
        elif name.endswith(".dmg") and data[-512:-508] != b"koly":
            raise ValueError("Invalid DMG")
    return "\n".join(checksums) + "\n"


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--directory", type=Path, required=True)
    p.add_argument("--version", required=True)
    p.add_argument("--sha", required=True)
    a = p.parse_args()
    checksums = verify(a.directory, a.version, a.sha)
    (ROOT / "target").mkdir(exist_ok=True)
    (ROOT / "target/SHA256SUMS.txt").write_text(checksums)
    (ROOT / "target/release-notes.md").write_text(
        "Meshcraft " + a.version + "\n\nSource: `" + a.sha + "`.\n\n"
        "Portable ZIP/tar.gz archives for Linux, Windows and macOS on x86_64 and ARM64; "
        "AppImage/Snap, MSI/MSIX and DMG installers. Linux portable/AppImage binaries target glibc 2.27 or older.\n\n"
        "Windows MSI/MSIX packages are unsigned; MSIX certificate signing is deferred. "
        "macOS bundles are ad-hoc signed, without Developer ID signing or notarization. "
        "Native GUI testing on Windows/macOS and ARM64 remains to be qualified.\n"
    )
    print(
        "Verified "
        + str(len(expected_assets(a.version)))
        + " primary release artifacts"
    )
