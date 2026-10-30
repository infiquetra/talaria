"""Shared folders for the packager tests."""

from __future__ import annotations

import os
import shutil
import tempfile
from contextlib import contextmanager
from pathlib import Path


def repo_root():
    return Path(__file__).resolve().parents[3]


@contextmanager
def scratch():
    previous = Path.cwd()
    directory = Path(tempfile.mkdtemp())
    try:
        os.chdir(directory)
        yield directory
    finally:
        os.chdir(previous)
        shutil.rmtree(directory)


@contextmanager
def prepend_path(directory):
    previous = os.environ.get("PATH", "")
    os.environ["PATH"] = str(directory) + os.pathsep + previous
    try:
        yield
    finally:
        os.environ["PATH"] = previous


@contextmanager
def without(name):
    previous = os.environ.pop(name, None)
    try:
        yield
    finally:
        if previous is not None:
            os.environ[name] = previous


def write_script(path, body):
    path = Path(path)
    path.write_text("#!/bin/sh\n" + body, encoding="utf-8")
    path.chmod(0o755)
    return path
