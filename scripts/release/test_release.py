"""Packaging contract tests use synthetic installer headers, never publish assets."""

import io
import json
import os
from pathlib import Path
import struct
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch
from contextlib import redirect_stdout
import zipfile
import common
import meta
import portable
import verify
import snap as snap_packager


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.sha = "a" * 40
        self.version = common.version()

    def archive(self, target="x86_64-pc-windows-msvc"):
        binary = self.root / "binary"
        data = bytearray(128)
        if "linux" in target:
            data[:6] = b"\x7fELF\x02\x01"
            struct.pack_into(
                "<H", data, 18, 183 if target.startswith("aarch64") else 62
            )
        elif "windows" in target:
            data[:2] = b"MZ"
            struct.pack_into("<I", data, 60, 64)
            data[64:68] = b"PE\0\0"
            struct.pack_into(
                "<H", data, 68, 0xAA64 if target.startswith("aarch64") else 0x8664
            )
        else:
            data[:4] = b"\xcf\xfa\xed\xfe"
            struct.pack_into(
                "<I", data, 4, 0x100000C if target.startswith("aarch64") else 0x1000007
            )
        binary.write_bytes(data)
        output = self.root / "assets"
        native = None
        if "linux" in target:
            native = self.root / ("native-" + target)
            native.mkdir(exist_ok=True)
            (native / "native-manifest.json").write_text(
                json.dumps(dict(target=target, minimumGlibc="2.27"))
            )
        with patch.dict(os.environ, MESHCRAFT_BUILD_SHA=self.sha), redirect_stdout(
            io.StringIO()
        ):
            return portable.assemble(target, binary, output, native)

    def test_portable_formats_are_reproducible_and_manifested(self):
        path = self.archive()
        digest = common.sha(path.read_bytes())
        self.archive()
        self.assertEqual(digest, common.sha(path.read_bytes()))
        for file in path.parent.glob("*"):
            if file.name.endswith((".tar.gz", ".zip")):
                verify.check_portable(
                    file.read_bytes(), file.name, self.version, self.sha
                )
                with self.assertRaisesRegex(ValueError, "source mismatch"):
                    verify.check_portable(
                        file.read_bytes(), file.name, self.version, "b" * 40
                    )
        package, _ = common.payload(path, self.root / "branding-check")
        self.assertEqual(
            (package / "meshcraft.png").read_bytes(),
            (common.ROOT / "packaging/meshcraft.png").read_bytes(),
        )
        self.assertTrue((package / "notices/branding.md").is_file())
        self.assertFalse((package / "meshcraft.svg").exists())

    def test_rejects_wrong_native_cpu_even_with_valid_metadata(self):
        archive = self.archive("x86_64-unknown-linux-gnu")
        package, _ = common.payload(archive, self.root / "expanded")
        data = (package / "bin/meshcraft").read_bytes()
        with self.assertRaisesRegex(ValueError, "architecture differs"):
            common.validate_binary("aarch64-unknown-linux-gnu", data)

    def test_snap_grade_allows_candidate_and_stable_for_both_architectures(self):
        for target, arch in (
            ("x86_64-unknown-linux-gnu", "amd64"),
            ("aarch64-unknown-linux-gnu", "arm64"),
        ):
            with self.subTest(target=target):
                archive = self.archive(target)
                output = self.root / ("snap-" + arch)

                def pack(args, **kwargs):
                    self.assertEqual(args[:2], ["snap", "pack"])
                    metadata = (Path(args[2]) / "meta/snap.yaml").read_text()
                    for line in [
                        "grade: stable",
                        "confinement: strict",
                        "base: core24",
                        "architectures: [" + arch + "]",
                    ]:
                        self.assertIn(line, metadata)
                    self.assertNotIn("grade: devel", metadata)
                    (
                        Path(args[3])
                        / ("meshcraft_" + self.version + "_" + arch + ".snap")
                    ).write_bytes(b"hsqs")

                arguments = ["snap.py", "--archive", str(archive), "--output", str(output)]
                with patch("sys.argv", arguments):
                    with patch("snap.subprocess.run", side_effect=pack):
                        with redirect_stdout(io.StringIO()):
                            snap_packager.main()
                artifact = output / ("meshcraft_" + self.version + "_" + arch + ".snap")
                metadata = json.loads(
                    Path(str(artifact) + ".manifest.json").read_text()
                )
                self.assertEqual(metadata["grade"], "stable")
                self.assertEqual(metadata["target"], target)

    def test_rejects_symlinked_input(self):
        binary = self.root / "binary"
        binary.write_bytes(b"payload")
        link = self.root / "linked"
        link.symlink_to(binary)
        with self.assertRaisesRegex(ValueError, "symlinked"):
            portable.assemble("x86_64-apple-darwin", link, self.root / "assets")

    def test_rejects_bad_app_version_and_inherited_version(self):
        app = self.root / "apps/desktop"
        app.mkdir(parents=True)
        for field in [
            "version.workspace = true",
            'version = "v1.2.3"',
            'version = "01.2.3"',
        ]:
            (app / "Cargo.toml").write_text("[package]\n" + field + "\n")
            with self.assertRaises(ValueError):
                common.version(self.root)

    def test_tag_dispatch_rejects_branches_mismatched_or_moved_tags(self):
        def git(*args):
            return subprocess.check_output(
                ["git", *args], cwd=self.root, text=True, stderr=subprocess.DEVNULL
            ).strip()

        git("init")
        git(
            "-c",
            "user.name=Tests",
            "-c",
            "user.email=tests@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "first",
        )
        git("tag", self.version)
        (self.root / "rust-toolchain.toml").write_text(
            '[toolchain]\nchannel = "1.99.1"\n'
        )
        with patch.object(meta, "ROOT", self.root):
            result = meta.metadata("refs/tags/" + self.version)
            self.assertEqual(result["sha"], git("rev-parse", "HEAD"))
            for ref in [
                "refs/heads/work",
                "refs/tags/v" + self.version,
                "refs/tags/99.0.0",
            ]:
                with self.assertRaises(ValueError):
                    meta.metadata(ref)
            git(
                "-c",
                "user.name=Tests",
                "-c",
                "user.email=tests@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "second",
            )
            with self.assertRaisesRegex(ValueError, "source differ"):
                meta.metadata("refs/tags/" + self.version)

    def test_payload_rejects_traversal_and_links(self):
        for name, kind in [
            ("../escape", tarfile.REGTYPE),
            ("package/link", tarfile.SYMTYPE),
        ]:
            archive = self.root / "bad.tar.gz"
            with tarfile.open(archive, "w:gz") as a:
                info = tarfile.TarInfo(name)
                info.type = kind
                info.linkname = "/tmp"
                a.addfile(info, io.BytesIO())
            with self.assertRaises(ValueError):
                common.payload(archive, self.root / "out")

    def test_payload_and_final_verifier_reject_modified_content(self):
        archive = self.archive("x86_64-unknown-linux-gnu")
        package, manifest = common.payload(archive, self.root / "expanded")
        self.assertEqual(manifest["sourceRevision"], self.sha)
        # Rebuild a valid container with a changed binary but an unchanged file manifest.
        bad = self.root / "bad.tar.gz"
        with tarfile.open(archive) as a, tarfile.open(bad, "w:gz") as out:
            for m in a.getmembers():
                data = a.extractfile(m).read()
                if m.name.endswith("/bin/meshcraft"):
                    data = data[:-1] + bytes([data[-1] ^ 0xFF])
                    m.size = len(data)
                out.addfile(m, io.BytesIO(data))
        with self.assertRaisesRegex(ValueError, "digest differs"):
            common.payload(bad, self.root / "bad")
        with self.assertRaisesRegex(ValueError, "digest mismatch"):
            verify.check_portable(
                bad.read_bytes(), archive.name, self.version, self.sha
            )

    def test_requires_complete_six_platform_matrix_and_checksums(self):
        for target in common.TARGETS:
            self.archive(target)
        assets = self.root / "assets"
        for name in verify.expected_assets(self.version):
            p = assets / name
            if p.exists():
                continue
            if name.endswith(".msix"):
                with zipfile.ZipFile(p, "w") as a:
                    a.writestr("AppxManifest.xml", "<Package/>")
                    a.writestr("AppxBlockMap.xml", "<BlockMap/>")
            elif name.endswith(".msi"):
                p.write_bytes(bytes.fromhex("d0cf11e0a1b11ae1"))
            elif name.endswith(".AppImage"):
                p.write_bytes(b"\x7fELF" + b"\0" * 4 + b"AI\x02" + b"\0" * 10)
            elif name.endswith(".snap"):
                p.write_bytes(b"hsqs" + b"\0" * 10)
            elif name.endswith(".dmg"):
                p.write_bytes(b"koly" + b"\0" * 508)
            with redirect_stdout(io.StringIO()):
                common.publish(p)
        self.assertEqual(
            22, len(verify.verify(assets, self.version, self.sha).splitlines())
        )
        victim = next(assets.glob("*.msi"))
        sidecar = Path(str(victim) + ".sha256")
        sidecar.write_text("0" * 64 + "  " + victim.name)
        with self.assertRaisesRegex(ValueError, "Invalid checksum"):
            verify.verify(assets, self.version, self.sha)
        victim.unlink()
        with self.assertRaisesRegex(ValueError, "matrix differs"):
            verify.verify(assets, self.version, self.sha)


if __name__ == "__main__":
    unittest.main()
