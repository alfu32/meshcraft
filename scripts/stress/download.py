#!/usr/bin/env python3
"""Fetch and verify an existing same-tag release executable. Never build or retag."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "release"))
from common import payload, validate_binary


def verify_download(archive, checksum, tag, source_revision, destination, target):
    expected = checksum.read_text().split()
    actual = hashlib.sha256(archive.read_bytes()).hexdigest()
    if len(expected) != 2 or expected[1] != archive.name or expected[0] != actual:
        raise ValueError("Release archive checksum/name mismatch")
    package, manifest = payload(archive, destination)
    for name, value in dict(
        version=tag, tag=tag, sourceRevision=source_revision, target=target
    ).items():
        if manifest.get(name) != value:
            raise ValueError(
                f"Release manifest {name} differs from selected tag/source/target"
            )
    native = package / "bin/meshcraft"
    validate_binary(target, native.read_bytes())
    binary = package / "meshcraft"
    if (
        subprocess.check_output([str(binary), "--version"], text=True).strip()
        != f"Meshcraft {tag}"
    ):
        raise ValueError("Release executable version differs from tag")
    return binary, dict(
        tag=tag,
        source_revision=source_revision,
        target=target,
        archive=archive.name,
        archive_sha256=actual,
        binary_sha256=hashlib.sha256(native.read_bytes()).hexdigest(),
        launcher_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
    )


def require_release(repo, tag, target):
    try:
        release = json.loads(
            subprocess.check_output(
                [
                    "gh",
                    "release",
                    "view",
                    tag,
                    "--repo",
                    repo,
                    "--json",
                    "tagName,isDraft,assets",
                ],
                text=True,
            )
        )
    except subprocess.CalledProcessError as error:
        raise ValueError(
            f"Published release {tag} is required; run Release at that tag first"
        ) from error
    if release["tagName"] != tag or release["isDraft"]:
        raise ValueError("A published release at the selected tag is required")
    name = f"meshcraft-{tag}-{target}.tar.gz"
    available = {entry["name"] for entry in release["assets"]}
    if not {name, name + ".sha256"} <= available:
        raise ValueError(
            f"Missing same-tag release binary/checksum: {name}; run Release first"
        )
    return name


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument(
        "--check-only",
        action="store_true",
        help="Fail before desktop setup if the published binary is absent",
    )
    p.add_argument("--repo", required=True)
    p.add_argument("--tag", required=True)
    p.add_argument("--source-revision", required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument(
        "--target",
        default="x86_64-unknown-linux-gnu",
        choices=["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"],
    )
    a = p.parse_args()
    if not re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", a.tag):
        p.error("Select a numeric existing version tag")
    a.output.mkdir(parents=True, exist_ok=True)
    name = require_release(a.repo, a.tag, a.target)
    if a.check_only:
        print("Published binary/checksum present: " + name)
        return
    subprocess.run(
        [
            "gh",
            "release",
            "download",
            a.tag,
            "--repo",
            a.repo,
            "--pattern",
            name,
            "--pattern",
            name + ".sha256",
            "--dir",
            str(a.output),
        ],
        check=True,
    )
    binary, metadata = verify_download(
        a.output / name,
        a.output / (name + ".sha256"),
        a.tag,
        a.source_revision,
        a.output / "payload",
        a.target,
    )
    (a.output / "provenance.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(binary.resolve())


if __name__ == "__main__":
    try:
        main()
    except (ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Release prerequisite failed: {error}")
