#!/usr/bin/env python3
"""Build and verify native DEB/RPM installers from an audited Linux payload."""

import argparse
import io
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import stat
import subprocess
import tarfile
import tempfile

from common import ROOT, payload, publish, safe, sha, validate_binary

ARCHITECTURES = {
    "x86_64-unknown-linux-gnu": ("amd64", "x86_64"),
    "aarch64-unknown-linux-gnu": ("arm64", "aarch64"),
}
LAUNCHER = b'#!/bin/sh\nexec /opt/meshcraft/meshcraft "$@"\n'
INTEGRATION = {
    "usr/share/applications/meshcraft.desktop": "packaging/linux/meshcraft.desktop",
    "usr/share/pixmaps/meshcraft.png": "packaging/meshcraft.png",
    "usr/share/mime/packages/meshcraft.xml": "packaging/linux/meshcraft-mime.xml",
    "usr/share/metainfo/io.github.alfu32.meshcraft.metainfo.xml": "packaging/linux/meshcraft.metainfo.xml",
}


def command(args, **kwargs):
    try:
        return subprocess.check_output([str(arg) for arg in args], **kwargs)
    except subprocess.CalledProcessError as error:
        output = error.output or b""
        if isinstance(output, bytes):
            output = output.decode("utf-8", errors="replace")
        raise ValueError(str(args[0]) + " failed: " + output) from error


def stage_payload(package, root):
    shutil.copytree(package, root / "opt/meshcraft")
    (root / "usr/bin").mkdir(parents=True)
    (root / "usr/bin/meshcraft").write_bytes(LAUNCHER)
    (root / "usr/bin/meshcraft").chmod(0o755)
    for destination, source in INTEGRATION.items():
        path = root / destination
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / source, path)
    notice = root / "usr/share/doc/meshcraft/copyright"
    notice.parent.mkdir(parents=True)
    notice.write_text(
        "Meshcraft: MIT OR Apache-2.0\n"
        "See /opt/meshcraft/LICENSE-MIT and LICENSE-APACHE.\n"
        "Bundled dependency licenses: /opt/meshcraft/notices/.\n"
    )
    root.chmod(0o755)
    for path in root.rglob("*"):
        path.chmod(0o755 if path.is_dir() or path.stat().st_mode & 0o111 else 0o644)


def build_deb(root, manifest, output):
    arch = ARCHITECTURES[manifest["target"]][0]
    control = root / "DEBIAN"
    control.mkdir()
    size = sum(p.stat().st_size for p in root.rglob("*") if p.is_file())
    (control / "control").write_text(
        "Package: meshcraft\n"
        "Version: {version}-1\n"
        "Architecture: {arch}\n"
        "Maintainer: Meshcraft contributors <alfu32@users.noreply.github.com>\n"
        "Section: graphics\nPriority: optional\n"
        "Homepage: https://github.com/alfu32/meshcraft\n"
        "Installed-Size: {size}\n"
        "Depends: libc6 (>= 2.27), libgcc-s1 | libgcc1, libstdc++6, "
        "libgl1, libegl1, libopengl0, libvulkan1\n"
        "Recommends: mesa-vulkan-drivers\n"
        "Description: Direct mesh modeler with dynamic and scripted objects\n"
        " Meshcraft provides direct mesh construction, architectural tools,\n"
        " reusable object prototypes, JavaScript scripting and procedural materials.\n"
        .format(version=manifest["version"], arch=arch, size=(size + 1023) // 1024)
    )
    artifact = output / ("meshcraft_" + manifest["version"] + "-1_" + arch + ".deb")
    command(["dpkg-deb", "-Zxz", "--root-owner-group", "--build", root, artifact])
    return artifact


def build_rpm(root, manifest, output, work):
    arch = ARCHITECTURES[manifest["target"]][1]
    top = work / "rpmbuild"
    for name in ("BUILD", "BUILDROOT", "RPMS", "SOURCES", "SPECS", "SRPMS"):
        (top / name).mkdir(parents=True)
    spec = top / "SPECS/meshcraft.spec"
    spec.write_text(
        "Name: meshcraft\nVersion: {version}\nRelease: 1\n"
        "Summary: Direct mesh modeler with dynamic and scripted objects\n"
        "License: MIT OR Apache-2.0\nURL: https://github.com/alfu32/meshcraft\n"
        "ExclusiveArch: x86_64 aarch64\n"
        # The payload contains private libraries whose providers must not leak
        # into the distro's dependency resolution. List the unbundled ABI here.
        "AutoReqProv: no\n"
        "Requires: glibc >= 2.27, libgcc, libstdc++, /bin/sh, coreutils\n"
        "Requires: libGL.so.1()(64bit), libEGL.so.1()(64bit)\n"
        "Requires: libOpenGL.so.0()(64bit), libvulkan.so.1()(64bit)\n"
        # The executable and ELF closure are already audited and hashed. RPM
        # stripping/debug processing would invalidate that payload manifest.
        "%global __os_install_post %{{nil}}\n"
        "%global debug_package %{{nil}}\n"
        "%global _build_id_links none\n"
        "%global _binary_payload w7.xzdio\n"
        "%global use_source_date_epoch_as_buildtime 1\n"
        "%global clamp_mtime_to_source_date_epoch 1\n"
        "%description\nMeshcraft provides direct mesh construction, architectural\n"
        "tools, reusable object prototypes, scripting and procedural materials.\n"
        "%prep\n%build\n%install\n"
        "mkdir -p %{{buildroot}}\ncp -a {root}/. %{{buildroot}}/\n"
        "%files\n%defattr(-,root,root,-)\n"
        "/opt/meshcraft\n/usr/bin/meshcraft\n/usr/share/applications/meshcraft.desktop\n"
        "/usr/share/pixmaps/meshcraft.png\n/usr/share/mime/packages/meshcraft.xml\n"
        "/usr/share/metainfo/io.github.alfu32.meshcraft.metainfo.xml\n"
        "/usr/share/doc/meshcraft\n"
        .format(version=manifest["version"], arch=arch, root=shlex.quote(str(root)))
    )
    command([
        "rpmbuild", "-bb", "--target", arch,
        "--define", "_topdir " + str(top),
        "--buildroot", top / "BUILDROOT/meshcraft", spec,
    ], stderr=subprocess.STDOUT)
    name = "meshcraft-" + manifest["version"] + "-1." + arch + ".rpm"
    artifact = output / name
    shutil.copyfile(top / "RPMS" / arch / name, artifact)
    return artifact


def add_file(files, name, data, mode, uid, gid):
    while name.startswith("./"):
        name = name[2:]
    if name in ("", ".") and stat.S_ISDIR(mode):
        return
    safe(name)
    if uid != 0 or gid != 0:
        raise ValueError("Native package file is not root-owned: " + name)
    if stat.S_ISDIR(mode):
        return
    if not stat.S_ISREG(mode) or name in files:
        raise ValueError("Invalid native package file: " + name)
    files[name] = (data, bool(mode & 0o111))


def read_cpio(data):
    """Read RPM's newc payload without extracting paths onto the host filesystem."""
    files = {}
    offset = 0
    while offset + 110 <= len(data):
        header = data[offset:offset + 110]
        if header[:6] not in (b"070701", b"070702"):
            raise ValueError("Invalid RPM CPIO header")
        fields = [int(header[p:p + 8], 16) for p in range(6, 110, 8)]
        mode, uid, gid, size, name_size = fields[1], fields[2], fields[3], fields[6], fields[11]
        start = offset + 110
        end = start + name_size
        if name_size < 1 or end > len(data) or data[end - 1] != 0:
            raise ValueError("Invalid RPM CPIO filename")
        name = data[start:end - 1].decode("utf-8")
        start = (end + 3) & ~3
        end = start + size
        if end > len(data):
            raise ValueError("Truncated RPM CPIO file")
        value = data[start:end]
        offset = (end + 3) & ~3
        if header[:6] == b"070702" and sum(value) & 0xffffffff != fields[12]:
            raise ValueError("Invalid RPM CPIO checksum")
        if name == "TRAILER!!!":
            if size or any(data[offset:]):
                raise ValueError("Invalid RPM CPIO trailer")
            return files
        add_file(files, name, value, mode, uid, gid)
    raise ValueError("RPM CPIO trailer missing")


def contents(artifact):
    if artifact.suffix == ".deb":
        metadata = command([
            "dpkg-deb", "--show", "--showformat=${Package}\\n${Version}\\n${Architecture}\\n",
            artifact,
        ], text=True).splitlines()
        files = {}
        with tarfile.open(fileobj=io.BytesIO(command(["dpkg-deb", "--fsys-tarfile", artifact]))) as archive:
            for member in archive.getmembers():
                mode = member.mode | (stat.S_IFDIR if member.isdir() else stat.S_IFREG)
                if not (member.isdir() or member.isfile()):
                    raise ValueError("Unsupported DEB payload member")
                add_file(files, member.name,
                         archive.extractfile(member).read() if member.isfile() else b"",
                         mode, member.uid, member.gid)
        return metadata, files
    if artifact.suffix == ".rpm":
        metadata = command([
            "rpm", "-qp", "--queryformat", "%{NAME}\\n%{VERSION}-%{RELEASE}\\n%{ARCH}\\n",
            artifact,
        ], text=True).splitlines()
        return metadata, read_cpio(command(["rpm2cpio", artifact]))
    raise ValueError("Unsupported native package format")


def check_package(artifact, version, revision, target):
    arch = ARCHITECTURES[target][artifact.suffix == ".rpm"]
    metadata, files = contents(artifact)
    if metadata != ["meshcraft", version + "-1", arch]:
        raise ValueError("Native package name/version/architecture mismatch")
    manifest = json.loads(files["opt/meshcraft/package-manifest.json"][0])
    if (manifest["version"], manifest["tag"], manifest["sourceRevision"], manifest["target"]) != (
        version, version, revision, target
    ):
        raise ValueError("Native package source mismatch")
    expected = {"opt/meshcraft/" + name for name in manifest["files"]}
    expected.add("opt/meshcraft/package-manifest.json")
    if {name for name in files if name.startswith("opt/")} != expected:
        raise ValueError("Native package payload inventory mismatch")
    for name, entry in manifest["files"].items():
        safe(name)
        value, executable = files["opt/meshcraft/" + name]
        if (len(value), sha(value), executable) != (entry["size"], entry["sha256"], entry["executable"]):
            raise ValueError("Native package payload digest/mode mismatch: " + name)
    validate_binary(target, files["opt/meshcraft/bin/meshcraft"][0])
    if files.get("usr/bin/meshcraft") != (LAUNCHER, True):
        raise ValueError("Native package launcher mismatch")
    for name, source in INTEGRATION.items():
        if files.get(name) != ((ROOT / source).read_bytes(), False):
            raise ValueError("Native desktop integration mismatch: " + name)
    if set(files) != expected | set(INTEGRATION) | {"usr/bin/meshcraft", "usr/share/doc/meshcraft/copyright"}:
        raise ValueError("Unexpected native package files")
    return manifest


def build(archive, output, package_format="all"):
    if package_format not in ("all", "deb", "rpm"):
        raise ValueError("Unsupported native package format")
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="meshcraft-native-") as temp:
        work = Path(temp)
        package, manifest = payload(archive, work / "input")
        if manifest["target"] not in ARCHITECTURES:
            raise ValueError("DEB/RPM packages need a supported Linux payload")
        if not re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", manifest["version"]):
            raise ValueError("Native package needs a SemVer release version")
        if manifest["tag"] != manifest["version"]:
            raise ValueError("Native package tag/version mismatch")
        validate_binary(manifest["target"], (package / "bin/meshcraft").read_bytes())
        artifacts = []
        for selected in (("deb", "rpm") if package_format == "all" else (package_format,)):
            root = work / selected
            stage_payload(package, root)
            artifact = (build_deb(root, manifest, output.resolve()) if selected == "deb"
                        else build_rpm(root, manifest, output.resolve(), work))
            installed = check_package(artifact, manifest["version"], manifest["sourceRevision"], manifest["target"])
            if installed != manifest:
                raise ValueError("Native package manifest differs from its portable payload")
            publish(artifact, dict(
                format=selected, version=manifest["version"], packageRevision=1,
                target=manifest["target"], sourceRevision=manifest["sourceRevision"],
                minimumGlibc=manifest["minimumGlibc"],
                payloadArchiveSha256=sha(archive.read_bytes()),
            ))
            artifacts.append(artifact)
        return artifacts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--format", choices=("all", "deb", "rpm"), default="all")
    args = parser.parse_args()
    os.environ.setdefault("SOURCE_DATE_EPOCH", "1767225600")
    build(args.archive, args.output, args.format)


if __name__ == "__main__":
    main()
