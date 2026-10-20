#!/usr/bin/env python3
"""Release only the selected existing tag; never invent or move tags."""

import os
import re
import subprocess
from common import ROOT, version


def metadata(source_ref):
    app_version = version()
    if source_ref != "refs/tags/" + app_version:
        raise ValueError(
            "Run this workflow at refs/tags/"
            + app_version
            + "; selected ref: "
            + source_ref
        )
    head = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
    ).strip()
    tagged = subprocess.check_output(
        ["git", "rev-parse", source_ref + "^{commit}"], cwd=ROOT, text=True
    ).strip()
    if head != tagged:
        raise ValueError("Selected tag and checked-out source differ")
    toolchain = re.search(
        r'(?m)^channel\s*=\s*"([0-9.]+)"', (ROOT / "rust-toolchain.toml").read_text()
    )
    if not toolchain:
        raise ValueError("Rust toolchain must be pinned")
    epoch = subprocess.check_output(
        ["git", "show", "-s", "--format=%ct", "HEAD"], cwd=ROOT, text=True
    ).strip()
    return dict(
        version=app_version,
        tag=app_version,
        sha=head,
        toolchain=toolchain[1],
        epoch=epoch,
    )


if __name__ == "__main__":
    for k, v in metadata(os.environ.get("GITHUB_REF", "")).items():
        print(k + "=" + v)
