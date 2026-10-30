"""Compute disposable cache identities and save authority, without reading credentials."""

import argparse
import hashlib
import os
import re
import subprocess
import tomllib
from pathlib import Path

BUILD_PATHS = {
    "native-debug": "target/debug",
    "coverage-debug": "target/llvm-cov-target/debug",
    "bench-release": "target/release",
    "contract-check-debug": "target/debug",
    # cargo install keeps its original temporary build and locked installation.
    "contract-drift": "",
    "release-debug": "target/debug",
}


def digest(parts: list[bytes]) -> str:
    return hashlib.sha256(b"\0".join(parts)).hexdigest()


def plan(
    root: Path, role: str, env: dict[str, str], compiler: str, cargo: str
) -> dict[str, str]:
    if role not in BUILD_PATHS:
        raise ValueError("unknown compilation role")
    pin = (root / "rust-toolchain.toml").read_bytes()
    channel = tomllib.loads(pin.decode())["toolchain"]["channel"]
    if f"release: {channel}" not in compiler.splitlines():
        raise ValueError("active compiler differs from the pinned toolchain")
    platform = env["RUNNER_OS"] + "-" + env["RUNNER_ARCH"]
    if not re.fullmatch(r"[A-Za-z0-9_-]+", platform):
        raise ValueError("invalid runner platform")
    toolchain = digest([pin, compiler.encode(), cargo.encode()])
    lock = digest([(root / "Cargo.lock").read_bytes()])
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    files = {
        root / "Cargo.toml",
        *root.glob(".cargo/config*"),
        *root.glob(".github/workflows/*.yml"),
    }
    for member in manifest.get("workspace", {}).get("members", []):
        files.update(root.glob(member + "/Cargo.toml"))
    config = [
        Path(__file__).read_bytes(),
        Path(__file__).with_name("action.yml").read_bytes(),
    ]
    for path in sorted(files):
        if path.is_file():
            config.extend([str(path.relative_to(root)).encode(), path.read_bytes()])
    for name in sorted(env):
        if name in {
            "RUSTFLAGS",
            "RUSTDOCFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
        } or name.startswith(("CARGO_PROFILE_", "CARGO_BUILD_")):
            config.extend([name.encode(), env[name].encode()])
    prefix = f"cargo-v1-{platform}-{toolchain}-{lock}-{digest(config)}"
    event, ref = env.get("GITHUB_EVENT_NAME"), env.get("GITHUB_REF", "")
    default = env.get("DEFAULT_BRANCH", "")
    # Release checkouts can differ from a dispatch's branch ref; never seed that scope.
    trusted = (
        role != "release-debug"
        and bool(default)
        and ref == "refs/heads/" + default
        and event in {"push", "schedule", "workflow_dispatch"}
    )
    release_tag = (
        role == "release-debug"
        and event == "push"
        and bool(
            re.fullmatch(r"refs/tags/v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?", ref)
        )
    )
    return {
        "download-key": prefix + "-downloads",
        "build-key": prefix + "-" + role,
        "build-path": BUILD_PATHS[role],
        "cargo-home": env.get("CARGO_HOME") or str(Path.home() / ".cargo"),
        "save-downloads": str(trusted).lower(),
        "save-build": str(bool(BUILD_PATHS[role]) and (trusted or release_tag)).lower(),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("role", choices=BUILD_PATHS)
    args = parser.parse_args()
    compiler = subprocess.check_output(["rustc", "-vV"], text=True)
    cargo = subprocess.check_output(["cargo", "-vV"], text=True)
    names = {
        "RUNNER_OS",
        "RUNNER_ARCH",
        "GITHUB_EVENT_NAME",
        "GITHUB_REF",
        "DEFAULT_BRANCH",
        "CARGO_HOME",
        "RUSTFLAGS",
        "RUSTDOCFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
    }
    env = {
        name: os.environ[name]
        for name in os.environ
        if name in names or name.startswith(("CARGO_PROFILE_", "CARGO_BUILD_"))
    }
    outputs = plan(Path.cwd(), args.role, env, compiler, cargo)
    if any("\n" in value or "\r" in value for value in outputs.values()):
        raise ValueError("cache metadata must fit on single output lines")
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.writelines(f"{name}={value}\n" for name, value in outputs.items())


if __name__ == "__main__":
    main()
