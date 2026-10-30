"""Archive members, repeatable bytes, and refused inputs."""

from __future__ import annotations

import hashlib
import os
import tarfile
import unittest
from pathlib import Path

import package

from .support import scratch


_EPOCH = "1700000000"
_TARGET = "aarch64-apple-darwin"
_VERSION = "0.7.1"


def _root():
    root = Path("root")
    root.mkdir()
    (root / "LICENSE").write_bytes(b"license\n")
    (root / "README.md").write_bytes(b"readme\n")
    return root


def _binary(name="talaria-bin", mode=0o755):
    path = Path(name)
    path.write_bytes(b"bin\n")
    path.chmod(mode)
    return path


def _names(version, target):
    folder = f"talaria-{version}-{target}"
    return [f"{folder}/talaria", f"{folder}/LICENSE", f"{folder}/README.md"]


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        self._epoch = os.environ.get("SOURCE_DATE_EPOCH")

    def tearDown(self):
        if self._epoch is None:
            os.environ.pop("SOURCE_DATE_EPOCH", None)
        else:
            os.environ["SOURCE_DATE_EPOCH"] = self._epoch

    def test_archive_layout_modes_and_owners(self):
        with scratch():
            os.environ["SOURCE_DATE_EPOCH"] = _EPOCH
            root = _root()
            binary = _binary()
            out = Path("out")
            out.mkdir()
            result = package.archive(_TARGET, _VERSION, str(binary), str(out), str(root))
            self.assertTrue(result.path, "no archive command")
            archive = Path(result.path)
            self.assertEqual(
                archive.name,
                f"talaria-{_VERSION}-{_TARGET}.tar.gz",
                "no archive command",
            )
            with tarfile.open(archive, "r:gz") as packed:
                members = packed.getmembers()
                self.assertEqual(
                    [item.name for item in members],
                    _names(_VERSION, _TARGET),
                    "no archive command",
                )
                modes = (0o755, 0o644, 0o644)
                payloads = (b"bin\n", b"license\n", b"readme\n")
                for item, mode, payload in zip(members, modes, payloads):
                    self.assertTrue(item.isfile(), "no archive command")
                    self.assertEqual(item.mode & 0o777, mode, "no archive command")
                    self.assertEqual(item.uid, 0, "no archive command")
                    self.assertEqual(item.gid, 0, "no archive command")
                    self.assertEqual(item.uname, "", "no archive command")
                    self.assertEqual(item.gname, "", "no archive command")
                    self.assertEqual(item.mtime, int(_EPOCH), "no archive command")
                    extracted = packed.extractfile(item)
                    self.assertIsNotNone(extracted, "no archive command")
                    self.assertEqual(extracted.read(), payload, "no archive command")

    def test_archive_bytes_are_reproducible(self):
        with scratch():
            root = _root()
            binary = _binary()
            os.environ["SOURCE_DATE_EPOCH"] = _EPOCH
            first = Path("one")
            second = Path("two")
            third = Path("three")
            for folder in (first, second, third):
                folder.mkdir()
            left = package.archive(_TARGET, _VERSION, str(binary), str(first), str(root))
            right = package.archive(_TARGET, _VERSION, str(binary), str(second), str(root))
            self.assertTrue(left.path and right.path, "no archive command")
            left_bytes = Path(left.path).read_bytes()
            right_bytes = Path(right.path).read_bytes()
            self.assertEqual(
                hashlib.sha256(left_bytes).hexdigest(),
                hashlib.sha256(right_bytes).hexdigest(),
                "no archive command",
            )
            self.assertEqual(left_bytes[0:2], b"\x1f\x8b", "no archive command")
            self.assertEqual(left_bytes[4:8], b"\x00\x00\x00\x00", "no archive command")
            os.environ["SOURCE_DATE_EPOCH"] = str(int(_EPOCH) + 1)
            other = package.archive(_TARGET, _VERSION, str(binary), str(third), str(root))
            self.assertTrue(other.path, "no archive command")
            self.assertNotEqual(
                hashlib.sha256(left_bytes).hexdigest(),
                hashlib.sha256(Path(other.path).read_bytes()).hexdigest(),
                "no archive command",
            )

    def test_archive_refuses_bad_inputs(self):
        with scratch():
            root = _root()
            binary = _binary()
            plain = _binary("plain-bin", 0o644)
            os.environ["SOURCE_DATE_EPOCH"] = _EPOCH
            long_version = "0.0.0-" + ("a" * 52)
            self.assertEqual(len(long_version), 58)
            cases = (
                (
                    "long",
                    _TARGET,
                    long_version,
                    str(binary),
                    str(root),
                    _EPOCH,
                    "package.py: version too long for the archive format (at most 57 characters)",
                ),
                (
                    "missing-bin",
                    _TARGET,
                    _VERSION,
                    "missing-bin",
                    str(root),
                    _EPOCH,
                    "package.py: binary not found: missing-bin",
                ),
                (
                    "plain",
                    _TARGET,
                    _VERSION,
                    str(plain),
                    str(root),
                    _EPOCH,
                    "package.py: binary is not executable: plain-bin",
                ),
                (
                    "no-licence",
                    _TARGET,
                    _VERSION,
                    str(binary),
                    str(Path("empty-root")),
                    _EPOCH,
                    "package.py: LICENSE not found at the repository root",
                ),
                (
                    "unknown",
                    "x86_64-pc-windows-msvc",
                    _VERSION,
                    str(binary),
                    str(root),
                    _EPOCH,
                    "package.py: unknown target 'x86_64-pc-windows-msvc'",
                ),
                (
                    "bad-epoch",
                    _TARGET,
                    _VERSION,
                    str(binary),
                    str(root),
                    "nope",
                    "package.py: SOURCE_DATE_EPOCH must be a whole number of seconds",
                ),
            )
            Path("empty-root").mkdir()
            (Path("empty-root") / "README.md").write_bytes(b"readme\n")
            for folder, target, version, binary_path, root_path, epoch, message in cases:
                out = Path(folder)
                out.mkdir()
                before = sorted(item.name for item in out.iterdir())
                os.environ["SOURCE_DATE_EPOCH"] = epoch
                result = package.archive(target, version, binary_path, str(out), root_path)
                after = sorted(item.name for item in out.iterdir())
                self.assertEqual(result.code, 2, "no validation")
                self.assertIn(message, result.stderr, "no validation")
                self.assertEqual(after, before, "no validation")
            unknown = package.archive(
                "x86_64-pc-windows-msvc",
                _VERSION,
                str(binary),
                "unknown",
                str(root),
            )
            for target in (
                "aarch64-apple-darwin",
                "x86_64-apple-darwin",
                "x86_64-unknown-linux-musl",
                "aarch64-unknown-linux-musl",
            ):
                self.assertIn(target, unknown.stderr, "no validation")
            for epoch in ("", "-1", "1.5"):
                os.environ["SOURCE_DATE_EPOCH"] = epoch
                out = Path(f"epoch-{epoch or 'empty'}")
                out.mkdir()
                before = sorted(item.name for item in out.iterdir())
                result = package.archive(_TARGET, _VERSION, str(binary), str(out), str(root))
                after = sorted(item.name for item in out.iterdir())
                self.assertEqual(result.code, 2, "no validation")
                self.assertIn(
                    "package.py: SOURCE_DATE_EPOCH must be a whole number of seconds",
                    result.stderr,
                    "no validation",
                )
                self.assertEqual(after, before, "no validation")
