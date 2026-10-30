"""One real host build, only when the live switch is set."""

from __future__ import annotations

import os
import shutil
import subprocess
import tarfile
import unittest
from pathlib import Path

import package

from .support import repo_root, scratch


def _host_triple():
    chosen = os.environ.get("PACKAGE_HOST_TARGET")
    if chosen:
        return chosen
    probed = subprocess.run(
        ["rustc", "-vV"],
        capture_output=True,
        text=True,
        check=False,
    )
    for line in probed.stdout.splitlines():
        if line.startswith("host: "):
            return line.split(":", 1)[1].strip()
    return ""


def _package_target(host):
    if host in ("aarch64-apple-darwin", "x86_64-apple-darwin"):
        return host
    if host in ("x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl"):
        return "x86_64-unknown-linux-musl"
    if host.startswith("aarch64-unknown-linux-"):
        return "aarch64-unknown-linux-musl"
    return None


def _workspace_version():
    text = (repo_root() / "Cargo.toml").read_text(encoding="utf-8")
    for line in text.splitlines():
        if line.startswith("version = "):
            return line.split("=", 1)[1].strip().strip('"')
    return ""


class LiveBuildTests(unittest.TestCase):
    def setUp(self):
        self._epoch = os.environ.get("SOURCE_DATE_EPOCH")

    def tearDown(self):
        if self._epoch is None:
            os.environ.pop("SOURCE_DATE_EPOCH", None)
        else:
            os.environ["SOURCE_DATE_EPOCH"] = self._epoch

    def test_host_target_builds_packs_and_checks(self):
        if os.environ.get("PACKAGE_LIVE") != "1":
            self.skipTest("skipped unless PACKAGE_LIVE=1")
        host = _host_triple()
        target = _package_target(host)
        if target is None:
            self.skipTest(f"no package target for host {host}")
        if target.endswith("-linux-musl") and shutil.which("musl-gcc") is None:
            self.fail("musl-gcc not found; install musl-tools")
        built = package.build(target)
        self.assertTrue(built.path, "no build command")
        binary = Path(built.path)
        self.assertTrue(binary.is_file(), "no build command")
        version = _workspace_version()
        header = package.read_header(str(binary), target)
        self.assertTrue(header.ok, "no build command")
        checked = package.check_version(str(binary), version)
        self.assertTrue(checked.ok, "no build command")
        with scratch() as folder:
            os.environ["SOURCE_DATE_EPOCH"] = "1700000000"
            packed = package.archive(
                target,
                version,
                str(binary),
                "out",
                str(repo_root()),
            )
            self.assertTrue(packed.path, "no build command")
            with tarfile.open(packed.path, "r:gz") as archive:
                archive.extractall(path=folder / "unpacked")
            extracted = folder / "unpacked" / f"talaria-{version}-{target}" / "talaria"
            self.assertTrue(extracted.is_file(), "no build command")
            ran = subprocess.run(
                [str(extracted), "--version"],
                capture_output=True,
                text=True,
                check=False,
                timeout=10,
            )
            self.assertEqual(ran.returncode, 0, "no build command")
            base = version.split("-", 1)[0]
            printed = ran.stdout.strip()
            if "-" in version:
                self.assertTrue(printed.startswith(f"talaria {base}"), "no build command")
            else:
                self.assertEqual(printed, f"talaria {version}", "no build command")
