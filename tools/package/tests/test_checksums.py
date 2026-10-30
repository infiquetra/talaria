"""Checksum file text, refused archive sets, and upload planning."""

from __future__ import annotations

import hashlib
import shutil
import subprocess
import unittest
from pathlib import Path

import package

from .support import scratch


_ORDER = (
    "aarch64-apple-darwin",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-musl",
)
_MISSING_ORDER = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
)
_VERSION = "0.7.1"


def _archive_name(version, target):
    return f"talaria-{version}-{target}.tar.gz"


def _write_archives(folder, names, payload=b"x"):
    folder.mkdir(exist_ok=True)
    for name in names:
        (folder / name).write_bytes(payload)


def _expect(missing, extra):
    absent = ", ".join(missing) if missing else "none"
    more = ", ".join(extra) if extra else "none"
    return f"expected one archive per target; missing: {absent}; extra: {more}"


def _sums_text(pairs):
    lines = []
    for name, payload in pairs:
        digest = hashlib.sha256(payload).hexdigest()
        lines.append(f"{digest}  {name}")
    return "\n".join(lines) + "\n"


class ChecksumTests(unittest.TestCase):
    def test_sums_format_and_order(self):
        with scratch():
            folder = Path("archives")
            folder.mkdir()
            payloads = {}
            for index, target in enumerate(_ORDER):
                name = _archive_name(_VERSION, target)
                payloads[name] = bytes([index + 1])
                (folder / name).write_bytes(payloads[name])
            expected = _sums_text(
                (name, payloads[name]) for name in sorted(payloads)
            )
            result = package.checksums("archives")
            sums = folder / "SHA256SUMS"
            self.assertTrue(sums.is_file(), "no checksums command")
            self.assertEqual(result.code, 0, "no checksums command")
            self.assertEqual(sums.read_text(encoding="utf-8"), expected, "no checksums command")
            if shutil.which("shasum"):
                command = ["shasum", "-a", "256", "-c", "SHA256SUMS"]
            elif shutil.which("sha256sum"):
                command = ["sha256sum", "-c", "SHA256SUMS"]
            else:
                self.fail("no checksums command")
            checked = subprocess.run(
                command,
                cwd=folder,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(checked.returncode, 0, "no checksums command")

    def test_sums_refuse_wrong_archive_set(self):
        with scratch():
            four = [_archive_name(_VERSION, target) for target in _ORDER]
            _write_archives(Path("empty"), [])
            empty = package.checksums("empty")
            self.assertEqual(empty.code, 1, "no validation")
            self.assertIn(
                _expect(_MISSING_ORDER, []),
                empty.stderr,
                "no validation",
            )

            three = [name for name in four if "aarch64-unknown-linux-musl" not in name]
            _write_archives(Path("three"), three)
            short = package.checksums("three")
            self.assertEqual(short.code, 1, "no validation")
            self.assertIn(
                _expect(("aarch64-unknown-linux-musl",), []),
                short.stderr,
                "no validation",
            )

            extra_name = "talaria-0.7.1-extra.tar.gz"
            _write_archives(Path("five"), four + [extra_name])
            extra = package.checksums("five")
            self.assertEqual(extra.code, 1, "no validation")
            self.assertIn(_expect([], [extra_name]), extra.stderr, "no validation")

            old = [
                _archive_name("0.7.0", "aarch64-apple-darwin"),
                _archive_name("0.7.0", "x86_64-apple-darwin"),
            ]
            current = [_archive_name(_VERSION, target) for target in _MISSING_ORDER]
            _write_archives(Path("mixed"), old + current)
            mixed = package.checksums("mixed")
            self.assertEqual(mixed.code, 1, "no validation")
            self.assertIn(
                _expect(
                    (
                        "x86_64-unknown-linux-musl",
                        "aarch64-unknown-linux-musl",
                    ),
                    sorted(current),
                ),
                mixed.stderr,
                "no validation",
            )

            unknown_name = "talaria-0.7.1-x86_64-pc-windows-msvc.tar.gz"
            _write_archives(Path("unknown"), [unknown_name])
            unknown = package.checksums("unknown")
            self.assertEqual(unknown.code, 1, "no validation")
            self.assertIn(
                _expect(_MISSING_ORDER, [unknown_name]),
                unknown.stderr,
                "no validation",
            )

    def test_plan_upload_never_replaces(self):
        with scratch():
            pairs = []
            for index, target in enumerate(_ORDER):
                pairs.append((_archive_name(_VERSION, target), bytes([index + 1])))
            sums_body = _sums_text(pairs)
            Path("SHA256SUMS").write_text(sums_body, encoding="utf-8")
            names = [name for name, _payload in pairs] + ["SHA256SUMS"]

            Path("empty").mkdir()
            fresh = package.plan_upload("SHA256SUMS", "empty")
            self.assertEqual(fresh.code, 0, "no plan-upload")
            self.assertEqual(fresh.stdout.splitlines(), names, "no plan-upload")

            subset = Path("subset")
            subset.mkdir()
            kept = pairs[0]
            (subset / kept[0]).write_bytes(kept[1])
            partial = package.plan_upload("SHA256SUMS", "subset")
            self.assertEqual(partial.code, 0, "no plan-upload")
            self.assertEqual(partial.stdout.splitlines(), names[1:], "no plan-upload")

            changed = Path("changed")
            changed.mkdir()
            (changed / kept[0]).write_bytes(b"other")
            differ = package.plan_upload("SHA256SUMS", "changed")
            self.assertEqual(differ.code, 1, "no plan-upload")
            self.assertEqual(differ.stdout, "", "no plan-upload")
            self.assertIn(
                "release v0.7.1 already has "
                f"{kept[0]} with a different checksum; not replacing it",
                differ.stderr,
                "no plan-upload",
            )

            extra = Path("extra")
            extra.mkdir()
            (extra / "notes.txt").write_text("nope\n", encoding="utf-8")
            unexpected = package.plan_upload("SHA256SUMS", "extra")
            self.assertEqual(unexpected.code, 1, "no plan-upload")
            self.assertEqual(unexpected.stdout, "", "no plan-upload")
            self.assertIn(
                "release v0.7.1 has an unexpected file notes.txt",
                unexpected.stderr,
                "no plan-upload",
            )
