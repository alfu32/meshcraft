#!/usr/bin/env python3
"""Create a publisher-matched local signing certificate and optionally upload CI secrets."""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import secrets
import subprocess

from common import ROOT
from msix import identity

SECRETS = ("MESHCRAFT_PFX_B64", "MESHCRAFT_PFX_PASSWORD")


def openssl(*arguments, data=None):
    result = subprocess.run(["openssl", *map(str, arguments)], input=data, capture_output=True)
    if result.returncode:
        raise ValueError("OpenSSL " + str(arguments[0]) + " failed (private values withheld)")
    return result.stdout


def prepare(directory):
    directory = directory.resolve()
    ignored = subprocess.run(["git", "check-ignore", "-q", str(directory / "meshcraft-signing.pfx")], cwd=ROOT)
    if ignored.returncode:
        raise ValueError("Signing directory must be git-ignored before creating private material")
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    directory.chmod(0o700)
    pfx = directory / "meshcraft-signing.pfx"
    password_file = directory / "password.txt"
    certificate = directory / "meshcraft-signing.pem"
    public = directory / "meshcraft-signing.cer"
    key = directory / "meshcraft-signing-key.pem"
    paths = (pfx, password_file, certificate, public, key)
    subject = identity()["publisher"]
    if not any(path.exists() for path in paths):
        password_file.write_text(secrets.token_urlsafe(48))
        password_file.chmod(0o600)
        openssl("req", "-new", "-x509", "-newkey", "rsa:3072", "-sha256", "-days", "1095",
                "-subj", "/" + subject, "-keyout", key, "-out", certificate,
                "-passout", "file:" + str(password_file),
                "-addext", "basicConstraints=critical,CA:FALSE",
                "-addext", "keyUsage=critical,digitalSignature",
                "-addext", "extendedKeyUsage=codeSigning")
    if not all(path.is_file() for path in (password_file, certificate, key)):
        raise ValueError("Incomplete signing directory; refusing to overwrite an existing key")
    actual = openssl("x509", "-in", certificate, "-subject", "-nameopt", "RFC2253", "-noout").decode().strip()
    if actual != "subject=" + subject:
        raise ValueError("Existing certificate does not match the Store publisher")
    openssl("x509", "-in", certificate, "-checkend", "86400", "-noout")
    if not pfx.exists() or not pfx.stat().st_size:
        openssl("pkcs12", "-export", "-in", certificate, "-inkey", key, "-out", pfx,
                "-passin", "file:" + str(password_file), "-passout", "stdin",
                "-name", "Meshcraft MSIX signing", "-keypbe", "AES-256-CBC", "-certpbe", "AES-256-CBC",
                "-macalg", "sha256", "-iter", "100000", data=password_file.read_bytes() + b"\n")
    expected = openssl("x509", "-in", certificate, "-outform", "DER")
    if not public.exists():
        public.write_bytes(expected)
    pfx_certificate = openssl("pkcs12", "-in", pfx, "-passin", "file:" + str(password_file), "-clcerts", "-nokeys")
    if public.read_bytes() != expected or openssl("x509", "-outform", "DER", data=pfx_certificate) != expected:
        raise ValueError("Public certificate differs from signing material")
    for path in paths:
        path.chmod(0o600)
    metadata = dict(subject=subject, sha256=hashlib.sha256(public.read_bytes()).hexdigest(),
                    validity=openssl("x509", "-in", certificate, "-dates", "-noout").decode().splitlines(),
                    secrets=list(SECRETS), certificateType="self-signed-code-signing")
    (directory / "certificate.json").write_text(json.dumps(metadata, indent=2) + "\n")
    (directory / "certificate.json").chmod(0o600)
    return pfx, password_file, public, metadata


def upload(pfx, password_file, repository):
    values = (base64.b64encode(pfx.read_bytes()), password_file.read_bytes())
    for name, value in zip(SECRETS, values):
        result = subprocess.run(["gh", "secret", "set", name, "--repo", repository],
                                input=value, capture_output=True)
        if result.returncode:
            raise ValueError("GitHub secret upload failed for " + name + " (private values withheld)")
        print("Uploaded repository secret: " + name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=ROOT / ".store/microsoft-signing")
    parser.add_argument("--upload-secrets", action="store_true")
    parser.add_argument("--repo", default="alfu32/meshcraft")
    args = parser.parse_args()
    previous_mask = os.umask(0o077)
    try:
        pfx, password, public, metadata = prepare(args.directory)
        print("Local signing backup: " + str(pfx.parent))
        print("Public certificate SHA-256: " + metadata["sha256"])
        if args.upload_secrets:
            upload(pfx, password, args.repo)
    finally:
        os.umask(previous_mask)


if __name__ == "__main__":
    main()
