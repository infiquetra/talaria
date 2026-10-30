"""Compute disposable cache identities and save authority, without reading credentials."""

import argparse
import hashlib
import json
import os
import re
import subprocess
import tomllib
from collections.abc import Mapping
from pathlib import Path

TYPIFY_ROLE = "typify-install-release"
TYPIFY_PATH = "target/ci-tools/cargo-typify-0.8.0"
# Published crates.io archive and its packaged Cargo.lock, independently checksum-verified.
TYPIFY_IDENTITY = {
    "package": "cargo-typify",
    "version": "0.8.0",
    "source": "registry+https://github.com/rust-lang/crates.io-index",
    "archive": "https://static.crates.io/crates/cargo-typify/cargo-typify-0.8.0.crate",
    "crate-sha256": "b8e2fa6a3db0bb3bbc12a1258610681d9c40c28a09f47b1cb2d904011acaf211",
    "lock-sha256": "31fdf69a045e913f57cf0c784570413c9a9baed743f2155718d52c380672af64",
    "features": "default",
    "profile": "release",
    "target": "host",
}

BUILD_PATHS = {
    "native-debug": "target/debug",
    "coverage-debug": "target/llvm-cov-target/debug",
    "bench-release": "target/release",
    "contract-check-debug": "target/debug",
    # Drift has no workspace objects; the separate tool role owns installation artifacts.
    "contract-drift": "",
    "release-debug": "target/debug",
    TYPIFY_ROLE: TYPIFY_PATH,
}

FLAGS = {"RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTFLAGS"}
BUILD_ENV = {
    "CARGO_BUILD_JOBS",
    "CARGO_BUILD_INCREMENTAL",
    "CARGO_BUILD_RUSTFLAGS",
    "CARGO_BUILD_RUSTDOCFLAGS",
}
CONTEXT = {
    "RUNNER_OS",
    "RUNNER_ARCH",
    "GITHUB_EVENT_NAME",
    "GITHUB_REF",
    "DEFAULT_BRANCH",
    "CARGO_HOME",
}
IMAGE_ENV = {"HOME", "ImageOS", "ImageVersion", "GITHUB_ACTIONS", "RUNNER_ENVIRONMENT"}


def unsupported_tool_override(name: str) -> bool:
    if name in FLAGS | BUILD_ENV | CONTEXT | IMAGE_ENV | {
        "CARGO_TERM_COLOR",
        "CARGO_NET_OFFLINE",
        "RUSTUP_HOME",
    }:
        return False
    if name.startswith("CARGO_PROFILE_") and "CODEGEN_BACKEND" not in name:
        return False
    return (
        name == "CARGO"
        or name.startswith(
            ("CARGO_", "RUSTC", "RUSTDOC", "RUSTUP_", "__CARGO_", "LLVM_")
        )
        or bool(
            re.match(
                r"(?i)(?:(?:HOST_|TARGET_)?(?:CC|CXX|CPP|AR|LD|NM|RANLIB|STRIP|OBJC|OBJCXX|"
                r"CFLAGS|CXXFLAGS|CPPFLAGS|LDFLAGS|ARFLAGS)(?:_|$)|"
                r"PKG_CONFIG|CMAKE_|CLANG_|BINDGEN_|CRATE_CC_|DYLD_)",
                name,
            )
        )
        or name
        in {
            "SDKROOT",
            "MACOSX_DEPLOYMENT_TARGET",
            "LIBRARY_PATH",
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "LD_AUDIT",
            "CPATH",
            "C_INCLUDE_PATH",
            "CPLUS_INCLUDE_PATH",
            "COMPILER_PATH",
            "GCC_EXEC_PREFIX",
            "MAKEFLAGS",
            "LIBCLANG_PATH",
        }
    )


def selected_environment(role: str, source: Mapping[str, str]) -> dict[str, str]:
    env = {}
    for name in tuple(source):
        if role == TYPIFY_ROLE and unsupported_tool_override(name):
            # Presence is enough; never read a registry token or other override value.
            env[name] = "present"
        elif (
            name in CONTEXT | FLAGS
            or name.startswith(("CARGO_PROFILE_", "CARGO_BUILD_"))
            or role == TYPIFY_ROLE
            and name in IMAGE_ENV
        ):
            env[name] = source[name]
    return env


def present(path: Path) -> bool:
    try:
        path.lstat()
    except FileNotFoundError:
        return False
    except OSError as error:
        raise ValueError("cannot inspect tool-cache path metadata") from error
    return True


def typify_config_home(root: Path, env: dict[str, str]) -> Path:
    home = Path(
        env.get("CARGO_HOME") or str(Path(env.get("HOME") or Path.home()) / ".cargo")
    )
    for base in (home, root / ".cargo"):
        if not present(base):
            continue
        if base.is_symlink():
            raise ValueError(
                "tool cache does not support aliased Cargo configuration directories"
            )
        try:
            if any(entry.name.startswith("config") for entry in base.iterdir()):
                raise ValueError(
                    "tool cache does not support Cargo configuration entries"
                )
        except OSError as error:
            raise ValueError(
                "cannot inspect Cargo configuration directory metadata"
            ) from error
    return home


def typify_identity(root: Path, env: dict[str, str]) -> bytes:
    if (
        env.get("GITHUB_ACTIONS") != "true"
        or env.get("RUNNER_ENVIRONMENT") != "github-hosted"
        or not re.fullmatch(r"[a-z0-9-]+", env.get("ImageOS", ""))
        or not re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", env.get("ImageVersion", ""))
    ):
        raise ValueError("tool cache requires hosted runner image metadata")
    if any(unsupported_tool_override(name) for name in env):
        raise ValueError("unsupported tool-cache environment override")
    for name in FLAGS | {"CARGO_BUILD_RUSTFLAGS", "CARGO_BUILD_RUSTDOCFLAGS"}:
        if re.search(
            r"@|--(?:target|sysroot|extern)|linker|link-arg|codegen-backend|llvm-args|profile-use|target-cpu|target-feature|(?:^|[\s\x1f])-L",
            env.get(name, ""),
        ):
            raise ValueError("unsupported compiler or native-tool flags")
    home = typify_config_home(root, env)
    target = root / TYPIFY_PATH
    if BUILD_PATHS.get(TYPIFY_ROLE) != TYPIFY_PATH:
        raise ValueError("tool cache must use its fixed compilation path")
    for path in (root, home, target):
        if (
            not path.is_absolute()
            or path.resolve() != path
            or "\n" in str(path)
            or "\r" in str(path)
            or present(path)
            and not path.is_dir()
        ):
            raise ValueError("tool cache requires canonical absolute paths")
    for path in (root / "target", root / "target/ci-tools", target):
        if path.is_symlink() or (present(path) and not path.is_dir()):
            raise ValueError("tool cache path is not a contained directory")
    protected = [
        home,
        *(
            root / path
            for role, path in BUILD_PATHS.items()
            if path and role != TYPIFY_ROLE
        ),
    ]
    if any(
        present(path) and path.is_symlink() or path.resolve() != path
        for path in protected
    ):
        raise ValueError("tool cache does not support aliased build or install paths")
    if any(
        target.is_relative_to(path) or path.is_relative_to(target) for path in protected
    ):
        raise ValueError(
            "tool compilation path overlaps installed or workspace build state"
        )
    identity = TYPIFY_IDENTITY | {
        name: env[name] for name in ("ImageOS", "ImageVersion")
    }
    return json.dumps(identity, sort_keys=True).encode()


def digest(parts: list[bytes]) -> str:
    return hashlib.sha256(b"\0".join(parts)).hexdigest()


def plan(
    root: Path, role: str, env: dict[str, str], compiler: str, cargo: str
) -> dict[str, str]:
    if role not in BUILD_PATHS:
        raise ValueError("unknown compilation role")
    tool = typify_identity(root, env) if role == TYPIFY_ROLE else None
    pin = (root / "rust-toolchain.toml").read_bytes()
    channel = tomllib.loads(pin.decode())["toolchain"]["channel"]
    if f"release: {channel}" not in compiler.splitlines():
        raise ValueError("active compiler differs from the pinned toolchain")
    if tool is not None:
        for metadata in (compiler, cargo):
            if not re.search(r"(?m)^host: [A-Za-z0-9_-]+$", metadata) or not re.search(
                r"(?m)^commit-hash: [0-9a-f]{40}$", metadata
            ):
                raise ValueError(
                    "tool cache requires complete compiler and Cargo identity"
                )
    platform = env["RUNNER_OS"] + "-" + env["RUNNER_ARCH"]
    if not re.fullmatch(r"[A-Za-z0-9_-]+", platform):
        raise ValueError("invalid runner platform")
    toolchain = digest([pin, compiler.encode(), cargo.encode()])
    lock = digest([(root / "Cargo.lock").read_bytes()])
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    if tool is not None:
        typify_config_home(root, env)
    # New-role refusal must not race into the legacy glob's content reads.
    config_paths = root.glob(".cargo/config*") if tool is None else ()
    files = {
        root / "Cargo.toml",
        *config_paths,
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
    build_key = prefix + ("-" + digest([tool]) if tool is not None else "") + "-" + role
    if tool is not None and len(build_key) > 512:
        raise ValueError("tool cache key exceeds the service limit")
    return {
        "download-key": prefix + "-downloads",
        "build-key": build_key,
        "build-path": BUILD_PATHS[role],
        "cargo-home": env.get("CARGO_HOME") or str(Path.home() / ".cargo"),
        "save-downloads": str(trusted and role != TYPIFY_ROLE).lower(),
        "save-build": str(bool(BUILD_PATHS[role]) and (trusted or release_tag)).lower(),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("role", choices=BUILD_PATHS)
    args = parser.parse_args()
    env = selected_environment(args.role, os.environ)
    if args.role == TYPIFY_ROLE:
        typify_identity(Path.cwd(), env)
    compiler = subprocess.check_output(["rustc", "-vV"], text=True)
    cargo = subprocess.check_output(["cargo", "-vV"], text=True)
    outputs = plan(Path.cwd(), args.role, env, compiler, cargo)
    if any("\n" in value or "\r" in value for value in outputs.values()):
        raise ValueError("cache metadata must fit on single output lines")
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.writelines(f"{name}={value}\n" for name, value in outputs.items())


if __name__ == "__main__":
    main()
