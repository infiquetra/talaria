"""Install page strings and the link from the project readme."""

from __future__ import annotations

import unittest

from .support import repo_root


def _rooted():
    coded = (
        (85, 115, 101, 114, 115),
        (104, 111, 109, 101),
        (112, 114, 105, 118, 97, 116, 101),
        (118, 97, 114),
        (116, 109, 112),
    )
    return tuple("/" + "".join(chr(item) for item in parts) for parts in coded)


class DocsTests(unittest.TestCase):
    def test_install_page_contents(self):
        root = repo_root()
        page = root / "docs" / "install.md"
        self.assertTrue(page.is_file(), "no page")
        text = page.read_text(encoding="utf-8")
        for literal in (
            "brew install infiquetra/talaria/talaria",
            "shasum -a 256 -c SHA256SUMS --ignore-missing",
            "sha256sum -c SHA256SUMS --ignore-missing",
            "gh attestation verify",
            "talaria-<version>-<target>.tar.gz",
            "SHA256SUMS",
            "aarch64-apple-darwin",
            "x86_64-apple-darwin",
            "x86_64-unknown-linux-musl",
            "aarch64-unknown-linux-musl",
        ):
            self.assertIn(literal, text, "no page")
        refused = [line for line in text.splitlines() if line.startswith("Do not")]
        self.assertEqual(len(refused), 1, "no page")
        for name in ("crates.io", "npm", "Python Package Index"):
            self.assertIn(name, refused[0], "no page")
        for line in text.splitlines():
            for prefix in _rooted():
                self.assertNotIn(prefix, line, "no page")
        self.assertNotIn("cargo install talaria", text, "no page")
        readme = (root / "README.md").read_text(encoding="utf-8")
        self.assertIn("docs/install.md", readme, "no page")
