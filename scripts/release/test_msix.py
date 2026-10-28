"""Manifest, package-integrity and private signing-material contract checks.

ZIP fixtures have synthetic CMS headers. Real CMS/deployment verification runs
on the native Windows CI runners, never against these test packages.
"""

import base64
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as ET
import zipfile

from common import ROOT, sha
import msix
import msix_signing


def certificate_fixture(directory):
    certificate = directory / "test.cer"
    subprocess.run([
        "openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-sha256",
        "-subj", "/" + msix.identity()["publisher"], "-outform", "DER",
        "-out", str(certificate), "-keyout", str(directory / "test.key"),
    ], check=True, capture_output=True)
    return certificate.read_bytes()


def package_fixture(version, arch, files):
    files = dict(files, **{"AppxManifest.xml": msix.manifest(version, arch)})
    namespace = "{http://schemas.microsoft.com/appx/2010/blockmap}"
    block_map = ET.Element(namespace + "BlockMap", HashMethod="http://www.w3.org/2001/04/xmlenc#sha256")
    for name, data in files.items():
        entry = ET.SubElement(block_map, namespace + "File", Name=name.replace("/", "\\"), Size=str(len(data)))
        for offset in range(0, len(data), 65536):
            digest = base64.b64encode(bytes.fromhex(sha(data[offset:offset + 65536]))).decode()
            ET.SubElement(entry, namespace + "Block", Hash=digest)
        if len(data) > 65536:
            ET.SubElement(entry, "{http://schemas.microsoft.com/appx/2021/blockmap}FileHash",
                          Hash=base64.b64encode(bytes.fromhex(sha(data))).decode())
    files["AppxBlockMap.xml"] = ET.tostring(block_map)
    files["AppxSignature.p7x"] = b"PKCX" + b"synthetic-CMS-fixture" * 8
    files["[Content_Types].xml"] = b"<Types/>"
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as archive:
        for name, data in files.items():
            archive.writestr(name, data)
    return buffer.getvalue()


class MsixTests(unittest.TestCase):
    def test_store_identity_both_architectures_and_version_mapping(self):
        store = msix.identity()
        for arch in ("x64", "arm64"):
            data = msix.manifest("0.41.1", arch)
            root = ET.fromstring(data)
            name = "{" + msix.NS[""] + "}"
            package = root.find(name + "Identity")
            self.assertEqual(package.get("Name"), "K3D0.Meshcraft-desktop")
            self.assertEqual(package.get("Publisher"), store["publisher"])
            self.assertEqual(package.get("ProcessorArchitecture"), arch)
            self.assertEqual(package.get("Version"), "1.41.1.0")
            self.assertEqual(root.find(name + "Properties/" + name + "PublisherDisplayName").text, "K3D0")
            self.assertIn(b".octd", data)
            self.assertIn(b"windows.appExecutionAlias", data)
            msix.check_manifest(data, "0.41.1", arch)
        self.assertEqual(msix.package_version("1.0.0"), "2.0.0.0")
        for version in ("v1.0.0", "0.01.0", "65535.0.0", "0.65536.0", "0.0.65536"):
            with self.assertRaises(ValueError):
                msix.package_version(version)

    def test_rejects_wrong_publisher_architecture_and_modified_payload_blocks(self):
        data = msix.manifest("0.41.1", "x64")
        wrong = data.replace(msix.identity()["publisher"].encode(), b"CN=Meshcraft")
        with self.assertRaisesRegex(ValueError, "Store identity"):
            msix.check_manifest(wrong, "0.41.1", "x64")
        with self.assertRaises(ValueError):
            msix.check_manifest(data, "0.41.1", "arm64")
        package = package_fixture("0.41.1", "x64", {"meshcraft.exe": b"payload" * 20000})
        with zipfile.ZipFile(io.BytesIO(package)) as archive:
            msix.check_package(archive, "0.41.1", "x64")
            changed = {name: archive.read(name) for name in archive.namelist()}
        block_map = ET.fromstring(changed["AppxBlockMap.xml"])
        file_hash = block_map.find(".//{http://schemas.microsoft.com/appx/2021/blockmap}FileHash")
        self.assertIsNotNone(file_hash)
        file_hash.set("Hash", "incorrect")
        altered = io.BytesIO()
        with zipfile.ZipFile(altered, "w") as archive:
            for name, value in changed.items():
                archive.writestr(name, ET.tostring(block_map) if name == "AppxBlockMap.xml" else value)
        with zipfile.ZipFile(io.BytesIO(altered.getvalue())) as archive:
            with self.assertRaisesRegex(ValueError, "whole-file digest"):
                msix.check_package(archive, "0.41.1", "x64")
        changed["meshcraft.exe"] = b"modified" * 17500
        output = io.BytesIO()
        with zipfile.ZipFile(output, "w") as archive:
            for name, value in changed.items():
                archive.writestr(name, value)
        with zipfile.ZipFile(io.BytesIO(output.getvalue())) as archive:
            with self.assertRaisesRegex(ValueError, "block-map"):
                msix.check_package(archive, "0.41.1", "x64")

    @unittest.skipUnless(shutil.which("openssl"), "OpenSSL is required for signing-material checks")
    def test_private_signing_backup_is_encrypted_reused_and_uploaded_through_stdin(self):
        (ROOT / "target").mkdir(exist_ok=True)
        previous_mask = os.umask(0o077)
        try:
            with tempfile.TemporaryDirectory(dir=ROOT / "target", prefix="test-msix-key-") as temp:
                directory = Path(temp)
                pfx, password, public, metadata = msix_signing.prepare(directory)
                original = pfx.read_bytes()
                self.assertIn("ENCRYPTED PRIVATE KEY", (directory / "meshcraft-signing-key.pem").read_text())
                self.assertEqual(metadata["subject"], msix.identity()["publisher"])
                msix.check_certificate(public.read_bytes())
                if os.name != "nt":
                    self.assertEqual(stat.S_IMODE(password.stat().st_mode), 0o600)
                    self.assertEqual(stat.S_IMODE(pfx.stat().st_mode), 0o600)
                msix_signing.prepare(directory)
                self.assertEqual(pfx.read_bytes(), original)
                log = io.StringIO()
                with patch.object(msix_signing.subprocess, "run") as run, redirect_stdout(log):
                    run.return_value.returncode = 0
                    msix_signing.upload(pfx, password, "owner/repository")
                for call, name in zip(run.call_args_list, msix_signing.SECRETS):
                    self.assertEqual(call.args[0], ["gh", "secret", "set", name, "--repo", "owner/repository"])
                    self.assertTrue(call.kwargs["input"])
                    self.assertNotIn(password.read_text(), log.getvalue())
                    self.assertNotIn(base64.b64encode(original).decode(), log.getvalue())
        finally:
            os.umask(previous_mask)


if __name__ == "__main__":
    unittest.main()
