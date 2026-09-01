"""Shared safety and provenance helpers for the Talaria v0.5.0 harness."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
from pathlib import Path
from typing import Any

RELEASE_VERSION = "0.5.0"
PRIMARY_MODEL_ROUTE = "opencode-go / muse-spark-1.2-contributor"
FALLBACK_MODEL_ROUTE = "ollama (ollama-cloud) / glm-5.3-flash"
TESTERS = frozenset({"talaria-t1", "talaria-t2"})
VERDICTS = frozenset({"pass", "fail", "blocked", "reserved"})
TERMINAL_VERDICTS = frozenset({"pass", "reserved"})
FALLBACK_REASON_CODES = frozenset(
    {
        "primary-unavailable",
        "connection-failure",
        "model-not-found",
        "bounded-test-incompletion",
    }
)


class HarnessError(RuntimeError):
    """A contract violation that makes acceptance evidence untrustworthy."""


def require_object(value: Any, *, field: str) -> dict[str, Any]:
    """Return an object or raise one consistent harness contract error."""
    if not isinstance(value, dict):
        raise HarnessError(f"{field} must be an object")
    return value


def require_string(value: Any, *, field: str) -> str:
    """Return a non-empty string or raise one consistent contract error."""
    if not isinstance(value, str) or not value.strip():
        raise HarnessError(f"{field} must be a non-empty string")
    return value


def sha256_file(path: Path) -> str:
    """Return the lowercase SHA-256 digest of one file's exact bytes."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_json_object(path: Path) -> dict[str, Any]:
    """Read a JSON object, rejecting missing files and non-object roots."""
    try:
        value: Any = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise HarnessError(f"cannot read JSON object {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise HarnessError(f"{path}: expected a JSON object at the document root")
    return value


def write_json_object(path: Path, value: dict[str, Any], *, replace: bool = False) -> None:
    """Write stable JSON without silently replacing an evidence file."""
    path.parent.mkdir(parents=True, exist_ok=True)
    mode = "w" if replace else "x"
    try:
        with path.open(mode, encoding="utf-8") as handle:
            json.dump(value, handle, indent=2, sort_keys=True)
            handle.write("\n")
    except FileExistsError as exc:
        raise HarnessError(f"refusing to replace existing evidence file: {path}") from exc


def is_within(path: Path, directory: Path) -> bool:
    """Whether ``path`` resolves inside ``directory`` (or is the directory)."""
    try:
        path.resolve().relative_to(directory.resolve())
    except ValueError:
        return False
    return True


def validate_tester(tester: str) -> str:
    if tester not in TESTERS:
        allowed = ", ".join(sorted(TESTERS))
        raise HarnessError(f"unknown tester {tester!r}; expected one of: {allowed}")
    return tester


def validate_config_dir(config_dir: Path, *, scratch_root: Path) -> Path:
    """Reject the real config directory and config paths outside tester scratch."""
    resolved = config_dir.expanduser().resolve()
    real_config = (Path.home() / ".talaria").resolve()
    if is_within(resolved, real_config) or is_within(real_config, resolved):
        raise HarnessError(f"refusing to use the real Talaria config directory: {resolved}")
    if not is_within(resolved, scratch_root):
        raise HarnessError(
            f"tester config directory {resolved} is outside scratch root {scratch_root.resolve()}"
        )
    return resolved


def isolated_environment(
    *,
    config_dir: Path,
    term: str,
    rows: int,
    columns: int,
    venv_bin: Path | None = None,
    monochrome: bool = False,
    terminal_program: str = "v050-pty-driver",
) -> dict[str, str]:
    """Build the child environment without source or operator Talaria overrides."""
    if not term.strip():
        raise HarnessError("TERM must be non-empty")
    if rows < 1 or columns < 1:
        raise HarnessError("terminal rows and columns must both be positive")

    environment = dict(os.environ)
    for name in tuple(environment):
        if name.startswith(("TALARIA_", "TEXTUAL_")):
            environment.pop(name)
    environment.pop("PYTHONHOME", None)
    environment.pop("PYTHONPATH", None)
    for name in (
        "NO_COLOR",
        "FORCE_COLOR",
        "CLICOLOR",
        "CLICOLOR_FORCE",
        "COLORTERM",
        "COLUMNS",
        "ESCDELAY",
        "LC_TERMINAL",
        "LINES",
        "ROWS",
        "TERMINFO",
        "TERM_PROGRAM",
    ):
        environment.pop(name, None)
    # This legacy Hermes value must never become an accidental live credential
    # source for an acceptance process.
    environment.pop("HERMES_DASHBOARD_SESSION_TOKEN", None)
    environment.update(
        TALARIA_CONFIG_DIR=str(config_dir),
        TERM=term,
        TERM_PROGRAM=terminal_program,
    )
    if monochrome:
        environment["NO_COLOR"] = "1"
    elif "256color" in term:
        environment["COLORTERM"] = "truecolor"
    if venv_bin is not None:
        current_path = environment.get("PATH", os.defpath)
        environment["PATH"] = f"{venv_bin}{os.pathsep}{current_path}"
        environment["VIRTUAL_ENV"] = str(venv_bin.parent)
    else:
        environment.pop("VIRTUAL_ENV", None)
    return environment


def run_command(
    argv: list[str],
    *,
    cwd: Path,
    environment: dict[str, str] | None = None,
    timeout: float = 120.0,
) -> subprocess.CompletedProcess[str]:
    """Run an argument array and retain both streams for an evidence receipt."""
    try:
        return subprocess.run(
            argv,
            cwd=cwd,
            env=environment,
            text=True,
            capture_output=True,
            timeout=timeout,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise HarnessError(f"command could not complete: {argv!r}: {exc}") from exc


def command_output(argv: list[str], *, cwd: Path) -> str:
    result = run_command(argv, cwd=cwd, timeout=30.0)
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip() or "no output"
        raise HarnessError(f"command failed ({result.returncode}): {argv!r}: {detail}")
    return result.stdout.strip()


_INSTALLED_IDENTITY_PROBE = r"""
import hashlib
import importlib.metadata
import json
import pathlib
import sys

import talaria

distribution = importlib.metadata.distribution("talaria")
direct_url_text = distribution.read_text("direct_url.json")
installed_files = []
for entry in distribution.files or ():
    path = pathlib.Path(distribution.locate_file(entry)).resolve()
    if not path.is_file():
        continue
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    installed_files.append((str(entry), digest))
installed_files.sort()
installed_files_sha256 = hashlib.sha256(
    json.dumps(installed_files, separators=(",", ":")).encode("utf-8")
).hexdigest()
print(json.dumps({
    "base_prefix": sys.base_prefix,
    "direct_url": json.loads(direct_url_text) if direct_url_text else None,
    "distribution_root": str(pathlib.Path(distribution.locate_file("")).resolve()),
    "distribution_version": distribution.version,
    "installed_file_count": len(installed_files),
    "installed_files_sha256": installed_files_sha256,
    "package_file": str(pathlib.Path(talaria.__file__).resolve()),
    "package_version": talaria.__version__,
    "prefix": sys.prefix,
    "python": sys.executable,
}, sort_keys=True))
"""


def validate_wheel_direct_url(direct_url: Any, *, wheel_sha256: str | None) -> None:
    """Reject editable, directory, and non-wheel installation provenance."""
    if not isinstance(direct_url, dict):
        raise HarnessError("installed distribution has no wheel direct_url.json provenance")
    directory_info = direct_url.get("dir_info")
    if isinstance(directory_info, dict) and directory_info.get("editable") is True:
        raise HarnessError("refusing an editable Talaria installation")
    url = direct_url.get("url")
    if not isinstance(url, str) or not url.lower().endswith(".whl"):
        raise HarnessError(f"Talaria was not installed from a wheel: {url!r}")
    if wheel_sha256 is not None:
        archive_info = direct_url.get("archive_info")
        installed_hash = archive_info.get("hash") if isinstance(archive_info, dict) else None
        if installed_hash != f"sha256={wheel_sha256}":
            raise HarnessError(
                "installed wheel hash does not match the frozen candidate: "
                f"{installed_hash!r} != 'sha256={wheel_sha256}'"
            )


def probe_installed_artifact(
    *,
    venv: Path,
    executable: Path,
    work_dir: Path,
    environment: dict[str, str],
    expected_version: str,
    integration_tree: Path | None,
    wheel_sha256: str | None,
) -> dict[str, Any]:
    """Prove the command resolves to a non-editable wheel inside one fresh venv."""
    venv = venv.resolve()
    executable = executable.resolve()
    expected_executable = (venv / "bin" / "talaria").resolve()
    if executable != expected_executable or not executable.is_file():
        raise HarnessError(
            f"refusing non-venv or missing executable: expected {expected_executable}, "
            f"found {executable}"
        )
    if not is_within(executable, venv):
        raise HarnessError(f"refusing global executable outside tester venv: {executable}")

    python = venv / "bin" / "python"
    if not python.exists():
        raise HarnessError(f"tester virtual environment has no Python executable: {python}")
    result = run_command(
        [str(python), "-I", "-c", _INSTALLED_IDENTITY_PROBE],
        cwd=work_dir,
        environment=environment,
        timeout=30.0,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip() or "no output"
        raise HarnessError(f"installed-artifact identity probe failed: {detail}")
    try:
        identity: Any = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise HarnessError("installed-artifact identity probe returned invalid JSON") from exc
    if not isinstance(identity, dict):
        raise HarnessError("installed-artifact identity probe returned a non-object")

    if identity.get("prefix") != str(venv):
        raise HarnessError(
            f"identity probe ran outside the tester venv: sys.prefix={identity.get('prefix')!r}"
        )
    if identity.get("base_prefix") == identity.get("prefix"):
        raise HarnessError(
            "identity probe is using a global interpreter, not a virtual environment"
        )
    for field in ("package_file", "distribution_root"):
        value = identity.get(field)
        if not isinstance(value, str):
            raise HarnessError(f"installed {field} is missing: {value!r}")
        if integration_tree is not None and is_within(Path(value), integration_tree):
            raise HarnessError(f"source-checkout leakage detected in {field}: {value}")
        if not is_within(Path(value), venv):
            raise HarnessError(f"installed {field} escaped the tester venv: {value!r}")

    validate_wheel_direct_url(identity.get("direct_url"), wheel_sha256=wheel_sha256)

    for field in ("distribution_version", "package_version"):
        if identity.get(field) != expected_version:
            raise HarnessError(
                f"installed {field} is {identity.get(field)!r}, expected {expected_version!r}"
            )
    identity["executable"] = str(executable)
    identity["executable_sha256"] = sha256_file(executable)
    return identity
