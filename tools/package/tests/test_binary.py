"""Header bytes, which hosts can run a target, and the version line."""

from __future__ import annotations

import unittest
from pathlib import Path

import package

from .support import scratch, without, write_script


# section 6.3 offsets
_MACH_64 = bytes.fromhex("cffaedfe")
_MACH_32 = bytes.fromhex("cefaedfe")
_MACH_FAT = bytes.fromhex("cafebabe")
_ARM64 = 0x0100000C
_X86_64 = 0x01000007
_ELF_AARCH64 = 183
_ELF_X86_64 = 62


def _macho(magic, cpu):
    return magic + cpu.to_bytes(4, "little") + bytes(24)


def _elf(machine, interp):
    # section 6.3 offsets
    buf = bytearray(64 + 56)
    buf[0:4] = b"\x7fELF"
    buf[4] = 2
    buf[5] = 1
    buf[18:20] = machine.to_bytes(2, "little")
    buf[32:40] = (64).to_bytes(8, "little")
    buf[54:56] = (56).to_bytes(2, "little")
    buf[56:58] = (1).to_bytes(2, "little")
    kind = 3 if interp else 1
    buf[64:68] = kind.to_bytes(4, "little")
    return bytes(buf)


def _hosts():
    return (
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "x86_64-unknown-linux-gnu",
        "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-gnu",
        "aarch64-unknown-linux-musl",
    )


def _runs(target, host):
    allowed = {
        "aarch64-apple-darwin": {"aarch64-apple-darwin"},
        "x86_64-apple-darwin": {"x86_64-apple-darwin"},
        "x86_64-unknown-linux-musl": {
            "x86_64-unknown-linux-gnu",
            "x86_64-unknown-linux-musl",
        },
        "aarch64-unknown-linux-musl": {
            "aarch64-unknown-linux-gnu",
            "aarch64-unknown-linux-musl",
        },
    }
    return host in allowed[target]


class BinaryTests(unittest.TestCase):
    def test_header_reader_identifies_arch_and_static(self):
        with scratch():
            cases = (
                (
                    "arm64.macho",
                    _macho(_MACH_64, _ARM64),
                    "aarch64-apple-darwin",
                    ["arch: arm64 (Mach-O)"],
                    "",
                ),
                (
                    "x86.macho",
                    _macho(_MACH_64, _X86_64),
                    "x86_64-apple-darwin",
                    ["arch: x86_64 (Mach-O)"],
                    "",
                ),
                (
                    "arm.elf",
                    _elf(_ELF_AARCH64, False),
                    "aarch64-unknown-linux-musl",
                    ["arch: aarch64 (ELF)", "linkage: static"],
                    "",
                ),
                (
                    "x86.elf",
                    _elf(_ELF_X86_64, False),
                    "x86_64-unknown-linux-musl",
                    ["arch: x86_64 (ELF)", "linkage: static"],
                    "",
                ),
            )
            for name, blob, target, lines, _reason in cases:
                Path(name).write_bytes(blob)
                result = package.read_header(name, target)
                self.assertTrue(result.ok, "no header reader")
                self.assertEqual(result.lines, lines, "no header reader")
                self.assertEqual(result.reason, "", "no header reader")

            refusals = (
                (
                    "macho32",
                    _MACH_32 + bytes(28),
                    "aarch64-apple-darwin",
                    "not a 64-bit Mach-O file",
                ),
                (
                    "fat",
                    _MACH_FAT + bytes(28),
                    "aarch64-apple-darwin",
                    "universal Mach-O files are not shipped; build one architecture per target",
                ),
                (
                    "dynamic.elf",
                    _elf(_ELF_X86_64, True),
                    "x86_64-unknown-linux-musl",
                    "binary is dynamically linked (it names an interpreter); "
                    "musl targets must be static",
                ),
                (
                    "wrong-cpu.macho",
                    _macho(_MACH_64, _ARM64),
                    "x86_64-apple-darwin",
                    "binary is arm64, target x86_64-apple-darwin needs x86_64",
                ),
                (
                    "wrong-cpu.elf",
                    _elf(_ELF_AARCH64, False),
                    "x86_64-unknown-linux-musl",
                    "binary is aarch64, target x86_64-unknown-linux-musl needs x86_64",
                ),
                (
                    "notes.txt",
                    b"not a binary\n",
                    "aarch64-apple-darwin",
                    "not a Mach-O or ELF executable",
                ),
            )
            for name, blob, target, reason in refusals:
                Path(name).write_bytes(blob)
                result = package.read_header(name, target)
                self.assertFalse(result.ok, "no header reader")
                self.assertEqual(result.reason, reason, "no header reader")

    def test_can_run_on_host(self):
        hosts = _hosts()
        for target in (
            "aarch64-apple-darwin",
            "x86_64-apple-darwin",
            "x86_64-unknown-linux-musl",
            "aarch64-unknown-linux-musl",
        ):
            for host in hosts:
                self.assertEqual(
                    package.can_run_on_host(target, host),
                    _runs(target, host),
                    "no predicate",
                )

    def test_version_line_rules(self):
        with scratch():
            write_script(
                "ok.sh",
                "printf '%s\\n' \"$LANG\" > lang.txt\n"
                "if [ -n \"${HOME+x}\" ]; then printf 'set\\n' > pad.txt; fi\n"
                "printf '%s\\n' 'talaria 0.7.1'\n",
            )
            with without("HOME"):
                passed = package.check_version("ok.sh", "0.7.1")
            self.assertTrue(passed.ok, "no version check")
            lang = Path("lang.txt").read_text(encoding="utf-8")
            self.assertEqual(lang, "C.UTF-8\n", "no version check")
            self.assertFalse(Path("pad.txt").exists(), "no version check")

            write_script("dev.sh", "printf '%s\\n' 'talaria 0.7.1-dev'\n")
            final_dev = package.check_version("dev.sh", "0.7.1")
            self.assertFalse(final_dev.ok, "no version check")
            self.assertEqual(
                final_dev.message,
                "--version printed 'talaria 0.7.1-dev', expected 'talaria 0.7.1'",
                "no version check",
            )

            pre = package.check_version("dev.sh", "0.7.1-rc.1")
            self.assertTrue(pre.ok, "no version check")
            other = package.check_version("dev.sh", "0.8.0-rc.1")
            self.assertFalse(other.ok, "no version check")

            write_script("name.sh", "printf '%s\\n' 'talaria-cli 0.7.1'\n")
            named = package.check_version("name.sh", "0.7.1")
            self.assertEqual(
                named.message,
                "--version printed 'talaria-cli 0.7.1', expected 'talaria 0.7.1'",
                "no version check",
            )

            write_script(
                "two.sh",
                "printf '%s\\n' 'talaria 0.7.1'\nprintf '%s\\n' 'extra'\n",
            )
            two = package.check_version("two.sh", "0.7.1")
            self.assertEqual(
                two.message,
                "--version printed 'talaria 0.7.1?extra?', expected 'talaria 0.7.1'",
                "no version check",
            )

            write_script("empty.sh", "exit 0\n")
            empty = package.check_version("empty.sh", "0.7.1")
            self.assertEqual(
                empty.message,
                "--version printed '', expected 'talaria 0.7.1'",
                "no version check",
            )

            write_script("exit3.sh", "exit 3\n")
            exited = package.check_version("exit3.sh", "0.7.1")
            self.assertEqual(exited.message, "--version exited 3", "no version check")

            write_script("slow.sh", "sleep 20\n")
            slow = package.check_version("slow.sh", "0.7.1")
            self.assertEqual(
                slow.message,
                "--version did not finish within 10 s",
                "no version check",
            )
