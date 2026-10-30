"""Packager stub. Commands return an empty result until the real steps exist."""

from __future__ import annotations

import sys


class Result:
    """Outcome of one packager call."""

    def __init__(
        self,
        code=0,
        stdout="",
        stderr="",
        ok=False,
        lines=None,
        reason="",
        path=None,
        message="",
    ):
        self.code = code
        self.stdout = stdout
        self.stderr = stderr
        self.ok = ok
        self.lines = [] if lines is None else lines
        self.reason = reason
        self.path = path
        self.message = message


TAG_PATTERN = ""


def targets_text():
    return ""


def targets_json():
    return "[]"


def parse_release_tag(tag):
    return None


def version_gate(tag, workspace_version):
    return Result()


def read_header(path, target):
    return Result()


def can_run_on_host(target, host):
    return False


def check_version(path, version):
    return Result()


def archive(target, version, binary, out, root):
    return Result()


def checksums(directory):
    return Result()


def plan_upload(sums_path, existing_dir):
    return Result()


def formula(version, sums_path, base_url=None):
    return Result()


def build(target):
    return Result()


def main(argv=None):
    sys.stderr.write("package.py: no parser\n")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
