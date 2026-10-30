"""Build and verify deterministic Talaria release packages.

Only Python's standard library is required. Commands return structured results so
that the same validation is available to the CLI and to offline tests.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import re
import shutil
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class Result:
    code: int = 0
    stdout: str = ""
    stderr: str = ""
    ok: bool = False
    lines: list[str] = field(default_factory=list)
    reason: str = ""
    path: str | None = None
    message: str = ""


TARGETS = (
    {"target": "aarch64-apple-darwin", "runner": "macos-14", "kind": "native"},
    {"target": "x86_64-apple-darwin", "runner": "macos-14", "kind": "cross"},
    {"target": "x86_64-unknown-linux-musl", "runner": "ubuntu-24.04", "kind": "native"},
    {"target": "aarch64-unknown-linux-musl", "runner": "ubuntu-24.04-arm", "kind": "native"},
)
TARGET_NAMES = tuple(row["target"] for row in TARGETS)
VERSION_PATTERN = r"[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?"
TAG_PATTERN = "v" + VERSION_PATTERN
ROOT = Path(__file__).resolve().parents[2]


def _display(value, limit=80):
    return "".join(ch if ch.isprintable() else "?" for ch in str(value)[:limit])


def _error(message, code=1):
    return Result(code=code, stderr=f"package.py: {message}\n", reason=message, message=message)


def _invalid_version(version):
    return _error(f"invalid version '{_display(version, 40)}'", 2)


def _valid_version(version):
    return len(version) <= 57 and re.fullmatch(VERSION_PATTERN, version) is not None


def _unknown_target(target):
    return _error(f"unknown target '{_display(target, 80)}'; expected: {', '.join(TARGET_NAMES)}", 2)


def targets_text():
    return "".join(f"{r['target']} {r['runner']} {r['kind']}\n" for r in TARGETS)


def targets_json():
    return json.dumps(TARGETS, separators=(",", ":")) + "\n"


def parse_release_tag(tag):
    return tag[1:] if re.fullmatch(TAG_PATTERN, tag) and len(tag[1:]) <= 57 else None


def version_gate(tag, workspace_version):
    version = parse_release_tag(tag)
    if version is None:
        return _error(f"invalid release tag '{_display(tag, 40)}'", 2)
    if not _valid_version(workspace_version):
        return _invalid_version(workspace_version)
    if "-" not in version and version != workspace_version:
        return _error(f"final tag {tag} needs workspace version {version}, found {workspace_version}")
    if version.split("-", 1)[0] != workspace_version.split("-", 1)[0]:
        return _error(f"tag {tag} and workspace version {workspace_version} have different base versions")
    return Result(stdout=f"{version}\n", ok=True)


def read_header(path, target):
    if target not in TARGET_NAMES:
        return _unknown_target(target)
    try:
        with open(path, "rb") as binary:
            head = binary.read(4096)
            magic = head[:4]
            if magic in (b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf", b"\xbe\xba\xfe\xca", b"\xbf\xba\xfe\xca"):
                return _error("universal Mach-O files are not shipped; build one architecture per target")
            if magic == b"\xce\xfa\xed\xfe":
                return _error("not a 64-bit Mach-O file")
            if magic == b"\xcf\xfa\xed\xfe":
                if len(head) < 32:
                    return _error("not a 64-bit Mach-O file")
                cpu = struct.unpack_from("<I", head, 4)[0]
                arch = {0x0100000C: "arm64", 0x01000007: "x86_64"}.get(cpu)
                if arch is None:
                    return _error(f"unknown Mach-O CPU type 0x{cpu:x}")
                if not target.endswith("-apple-darwin"):
                    return _error(f"binary is Mach-O, target {target} needs ELF")
                expected = "arm64" if target.startswith("aarch64-") else "x86_64"
                if arch != expected:
                    return _error(f"binary is {arch}, target {target} needs {expected}")
                return Result(ok=True, lines=[f"arch: {arch} (Mach-O)"])
            if magic != b"\x7fELF":
                return _error("not a Mach-O or ELF executable")
            if len(head) < 64 or head[4:6] != b"\x02\x01":
                return _error("not a 64-bit little-endian ELF file")
            machine = struct.unpack_from("<H", head, 18)[0]
            arch = {62: "x86_64", 183: "aarch64"}.get(machine)
            if arch is None:
                return _error(f"unknown ELF machine {machine}")
            if not target.endswith("-linux-musl"):
                return _error(f"binary is ELF, target {target} needs Mach-O")
            expected = "aarch64" if target.startswith("aarch64-") else "x86_64"
            if arch != expected:
                return _error(f"binary is {arch}, target {target} needs {expected}")
            offset = struct.unpack_from("<Q", head, 32)[0]
            size, count = struct.unpack_from("<HH", head, 54)
            if count and size < 56:
                return _error("invalid ELF program headers")
            binary.seek(0, os.SEEK_END)
            length = binary.tell()
            if offset > length or count * size > length - offset:
                return _error("invalid ELF program headers")
            for index in range(count):
                binary.seek(offset + index * size)
                kind = binary.read(4)
                if len(kind) != 4:
                    return _error("invalid ELF program headers")
                if struct.unpack("<I", kind)[0] == 3:
                    return _error("binary is dynamically linked (it names an interpreter); musl targets must be static")
            return Result(ok=True, lines=[f"arch: {arch} (ELF)", "linkage: static"])
    except OSError as exc:
        return _error(f"cannot read binary: {_display(exc.strerror or 'I/O error')}")


def can_run_on_host(target, host):
    if target not in TARGET_NAMES:
        return False
    if target.endswith("-apple-darwin"):
        return host == target
    cpu = target.split("-", 1)[0]
    return host in (f"{cpu}-unknown-linux-gnu", f"{cpu}-unknown-linux-musl")


def check_version(path, version):
    if not _valid_version(version):
        return _invalid_version(version)
    env = {"PATH": os.environ.get("PATH", ""), "LANG": "C.UTF-8"}
    if "HOME" in os.environ:
        env["HOME"] = os.environ["HOME"]
    try:
        completed = subprocess.run(
            [str(Path(path).resolve()), "--version"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            timeout=10,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return _error("--version did not finish within 10 s")
    except OSError as exc:
        return _error(f"could not run --version: {_display(exc.strerror or 'I/O error')}")
    if completed.returncode:
        return _error(f"--version exited {completed.returncode}")
    output = completed.stdout.decode("utf-8", errors="replace")
    line = output[:-1] if output.endswith("\n") and output.count("\n") == 1 else output
    prefix = "talaria "
    actual = line[len(prefix):] if line.startswith(prefix) else ""
    valid = _valid_version(actual) and (
        actual == version if "-" not in version else actual.split("-", 1)[0] == version.split("-", 1)[0]
    )
    if not valid:
        return _error(f"--version printed '{_display(line)}', expected 'talaria {version}'")
    return Result(ok=True, message=line)


def archive(target, version, binary, out, root):
    if target not in TARGET_NAMES:
        return _unknown_target(target)
    if len(version) > 57:
        return _error("version too long for the archive format (at most 57 characters)", 2)
    if not _valid_version(version):
        return _invalid_version(version)
    root_path = Path(root)
    source = Path(binary)
    if not source.is_file() and not source.is_absolute():
        candidate = root_path / source
        if candidate.is_file():
            source = candidate
    if not source.is_file():
        return _error(f"binary not found: {_display(binary)}", 2)
    if not source.stat().st_mode & (stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH):
        return _error(f"binary is not executable: {_display(binary)}", 2)
    for name in ("LICENSE", "README.md"):
        if not (root_path / name).is_file():
            return _error(f"{name} not found at the repository root", 2)
    epoch_text = os.environ.get("SOURCE_DATE_EPOCH", "")
    if not re.fullmatch(r"[0-9]+", epoch_text):
        return _error("SOURCE_DATE_EPOCH must be a whole number of seconds", 2)
    epoch = int(epoch_text)
    folder_name = f"talaria-{version}-{target}"
    out_path = Path(out)
    destination = out_path / f"{folder_name}.tar.gz"
    if destination.exists():
        return _error(f"archive already exists: {_display(destination)}", 2)
    temp_path = None
    try:
        out_path.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(dir=out_path, prefix=".package-", delete=False) as temp:
            temp_path = Path(temp.name)
            with gzip.GzipFile(filename="", mode="wb", fileobj=temp, mtime=0, compresslevel=9) as gz:
                with tarfile.open(fileobj=gz, mode="w", format=tarfile.USTAR_FORMAT) as tar:
                    for name, source_path, mode in (
                        ("talaria", source, 0o755),
                        ("LICENSE", root_path / "LICENSE", 0o644),
                        ("README.md", root_path / "README.md", 0o644),
                    ):
                        info = tarfile.TarInfo(f"{folder_name}/{name}")
                        info.size = source_path.stat().st_size
                        info.mode = mode
                        info.uid = info.gid = 0
                        info.uname = info.gname = ""
                        info.mtime = epoch
                        with source_path.open("rb") as source_file:
                            tar.addfile(info, source_file)
        os.link(temp_path, destination)
    except (OSError, ValueError, OverflowError) as exc:
        return _error(f"could not create archive: {_display(str(exc))}", 2)
    finally:
        if temp_path is not None:
            temp_path.unlink(missing_ok=True)
    return Result(ok=True, path=str(destination), stdout=f"{destination}\n")


def _archive_parts(name):
    if not name.startswith("talaria-") or not name.endswith(".tar.gz"):
        return None
    body = name[len("talaria-"):-len(".tar.gz")]
    for target in TARGET_NAMES:
        suffix = "-" + target
        if body.endswith(suffix):
            version = body[:-len(suffix)]
            if _valid_version(version):
                return version, target
    return None


def _digest(path):
    sha = hashlib.sha256()
    with open(path, "rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            sha.update(chunk)
    return sha.hexdigest()


def checksums(directory):
    folder = Path(directory)
    try:
        files = sorted(p for p in folder.glob("talaria-*.tar.gz") if p.is_file())
        present, extra = {}, []
        version = None
        for path in files:
            parts = _archive_parts(path.name)
            if parts is None:
                extra.append(path.name)
                continue
            found_version, target = parts
            if version is None:
                version = found_version
            if found_version != version or target in present:
                extra.append(path.name)
            else:
                present[target] = path
        missing = [target for target in TARGET_NAMES if target not in present]
        if missing or extra:
            absent = ", ".join(missing) if missing else "none"
            more = ", ".join(extra) if extra else "none"
            return _error(f"expected one archive per target; missing: {absent}; extra: {more}")
        body = "".join(f"{_digest(path)}  {path.name}\n" for path in files)
        (folder / "SHA256SUMS").write_text(body, encoding="utf-8")
    except OSError as exc:
        return _error(f"could not write checksums: {_display(exc.strerror or 'I/O error')}", 2)
    return Result(ok=True)


def _read_sums(path):
    entries = []
    text = Path(path).read_text(encoding="utf-8")
    for line in text.splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9._\-]+\.tar\.gz)", line)
        if not match:
            raise ValueError("invalid checksum line")
        digest, name = match.groups()
        parts = _archive_parts(name)
        if parts is None:
            raise ValueError("invalid archive name")
        entries.append((name, digest, parts[0], parts[1]))
    return entries


def plan_upload(sums_path, existing_dir):
    try:
        entries = _read_sums(sums_path)
        if len(entries) != 4 or len({e[2] for e in entries}) != 1 or {e[3] for e in entries} != set(TARGET_NAMES):
            raise ValueError("wrong archive set")
        version = entries[0][2]
        expected = {name: digest for name, digest, _, _ in entries}
        expected["SHA256SUMS"] = _digest(sums_path)
        existing = Path(existing_dir)
        names = sorted(path.name for path in existing.iterdir())
        for name in names:
            if name not in expected:
                return _error(f"release v{version} has an unexpected file {name}")
            path = existing / name
            if not path.is_file() or _digest(path) != expected[name]:
                return _error(f"release v{version} already has {name} with a different checksum; not replacing it")
        needed = [name for name, _, _, _ in entries if name not in names]
        if "SHA256SUMS" not in names:
            needed.append("SHA256SUMS")
        return Result(ok=True, stdout="".join(name + "\n" for name in needed))
    except (OSError, UnicodeError, ValueError):
        return _error(f"{_display(sums_path)} is not a four-target SHA256SUMS", 2)


def formula(version, sums_path, base_url=None):
    if not _valid_version(version):
        return _invalid_version(version)
    base = base_url if base_url is not None else f"https://github.com/infiquetra/talaria/releases/download/v{version}"
    base = base[:-1] if base.endswith("/") else base
    if not re.fullmatch(r"(https://[A-Za-z0-9.-]+|file://)/[A-Za-z0-9._~/%-]*", base):
        return _error("unsafe base URL", 2)
    try:
        entries = _read_sums(sums_path)
    except (OSError, UnicodeError, ValueError):
        return _error("invalid SHA256SUMS", 2)
    digests = {}
    for _, digest, found_version, target in entries:
        if found_version != version:
            return _error(f"SHA256SUMS names version {found_version}, not {version}", 2)
        if target in digests:
            return _error(f"SHA256SUMS repeats archive for {target}", 2)
        digests[target] = digest
    for target in TARGET_NAMES:
        if target not in digests:
            return _error(f"SHA256SUMS has no archive for {target}")
    def stanza(target):
        return f'      url "{base}/talaria-{version}-{target}.tar.gz"\n      sha256 "{digests[target]}"\n'
    text = (
        'class Talaria < Formula\n'
        '  desc "Terminal client for the Hermes agent gateway"\n'
        '  homepage "https://github.com/infiquetra/talaria"\n'
        f'  version "{version}"\n'
        '  license "MIT"\n\n'
        '  on_macos do\n    on_arm do\n' + stanza("aarch64-apple-darwin") +
        '    end\n    on_intel do\n' + stanza("x86_64-apple-darwin") +
        '    end\n  end\n\n  on_linux do\n    on_arm do\n' + stanza("aarch64-unknown-linux-musl") +
        '    end\n    on_intel do\n' + stanza("x86_64-unknown-linux-musl") +
        '    end\n  end\n\n'
        '  def install\n    bin.install "talaria"\n  end\n\n'
        '  def caveats\n    <<~EOS\n'
        '      Talaria needs a running Hermes gateway, v0.22.3 or newer.\n'
        '      Run `talaria doctor` to check the connection and versions.\n'
        '    EOS\n  end\n\n'
        '  test do\n'
        '    assert_equal "talaria #{version}", shell_output("#{bin}/talaria --version").strip\n'
        '  end\nend\n'
    )
    return Result(ok=True, stdout=text)


def build(target):
    if target not in TARGET_NAMES:
        return _unknown_target(target)
    if target.endswith("-linux-musl") and shutil.which("musl-gcc") is None:
        return _error("musl-gcc not found; install musl-tools", 2)
    try:
        installed = subprocess.run(["rustup", "target", "list", "--installed"], capture_output=True, text=True, check=False)
        if installed.returncode:
            return _error(f"rustup failed (exit {installed.returncode})", 2)
        if target not in installed.stdout.splitlines():
            added = subprocess.run(["rustup", "target", "add", target], stdout=sys.stderr, check=False)
            if added.returncode:
                return _error(f"rustup failed (exit {added.returncode})", 2)
        env = os.environ.copy()
        env["CARGO_PROFILE_RELEASE_STRIP"] = "symbols"
        env["CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP"] = "none"
        built = subprocess.run(["cargo", "build", "--release", "--locked", "-p", "talaria-cli", "--target", target], stdout=sys.stderr, env=env, check=False)
        if built.returncode:
            return _error(f"cargo build failed for {target} (exit {built.returncode})")
    except OSError as exc:
        return _error(f"build tool unavailable: {_display(exc.strerror or 'I/O error')}", 2)
    path = Path(os.environ.get("CARGO_TARGET_DIR", "target")) / target / "release" / "talaria"
    return Result(ok=True, path=str(path), stdout=f"{path}\n")


def _host_target():
    override = os.environ.get("PACKAGE_HOST_TARGET")
    if override:
        return override
    try:
        result = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=False)
    except OSError:
        return ""
    return next((line[6:].strip() for line in result.stdout.splitlines() if line.startswith("host: ")), "")


def check_binary(target, version, path):
    if not _valid_version(version):
        return _invalid_version(version)
    header = read_header(path, target)
    if not header.ok:
        return header
    lines = list(header.lines)
    if can_run_on_host(target, _host_target()):
        checked = check_version(path, version)
        if not checked.ok:
            return checked
        lines.append(f"version: {checked.message}")
    else:
        lines.append("version: not run (cannot run on this host)")
    return Result(ok=True, stdout="".join(line + "\n" for line in lines), lines=lines)


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise ValueError(message)


def main(argv=None):
    parser = Parser(prog="package.py")
    commands = parser.add_subparsers(dest="command", required=True, parser_class=Parser)
    targets_parser = commands.add_parser("targets")
    targets_parser.add_argument("--json", action="store_true")
    build_parser = commands.add_parser("build")
    build_parser.add_argument("--target", required=True)
    binary_parser = commands.add_parser("check-binary")
    binary_parser.add_argument("--target", required=True)
    binary_parser.add_argument("--version", required=True)
    binary_parser.add_argument("path")
    archive_parser = commands.add_parser("archive")
    archive_parser.add_argument("--target", required=True)
    archive_parser.add_argument("--version", required=True)
    archive_parser.add_argument("--binary", required=True)
    archive_parser.add_argument("--out", required=True)
    checksums_parser = commands.add_parser("checksums")
    checksums_parser.add_argument("directory")
    gate_parser = commands.add_parser("version-gate")
    gate_parser.add_argument("--tag", required=True)
    gate_parser.add_argument("--workspace-version", required=True)
    formula_parser = commands.add_parser("formula")
    formula_parser.add_argument("--version", required=True)
    formula_parser.add_argument("--sums", required=True)
    formula_parser.add_argument("--base-url")
    plan_parser = commands.add_parser("plan-upload")
    plan_parser.add_argument("sums")
    plan_parser.add_argument("existing")
    try:
        args = parser.parse_args(argv)
    except SystemExit as exc:
        return int(exc.code)
    except ValueError as exc:
        result = _error(_display(str(exc)), 2)
    else:
        if args.command == "targets":
            result = Result(ok=True, stdout=targets_json() if args.json else targets_text())
        elif args.command == "build":
            result = build(args.target)
        elif args.command == "check-binary":
            result = check_binary(args.target, args.version, args.path)
        elif args.command == "archive":
            result = archive(args.target, args.version, args.binary, args.out, ROOT)
        elif args.command == "checksums":
            result = checksums(args.directory)
        elif args.command == "version-gate":
            result = version_gate(args.tag, args.workspace_version)
        elif args.command == "formula":
            result = formula(args.version, args.sums, args.base_url)
        else:
            result = plan_upload(args.sums, args.existing)
    sys.stdout.write(result.stdout)
    sys.stderr.write(result.stderr)
    return result.code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
