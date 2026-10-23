"""Tests 14 to 17. Which legacy methods the native socket serves."""

from __future__ import annotations

import subprocess
import unittest
from pathlib import Path

import contract_gen

from . import support

NOT_NATIVE = [
    "agents.list",
    "session.activate",
    "session.archive",
    "session.branch",
    "session.branch_stored",
    "session.branch_whole",
    "session.close",
    "session.compress",
    "session.cwd.set",
    "session.delete",
    "session.set_hidden",
    "session.status",
    "session.title",
    "session.undo",
    "session.usage",
    "session.workspace.move",
]

HANDLER_MODULES = (
    "session_busy_controls",
    "session_queued_controls",
    "session_config",
    "session_tui_config",
    "session_ancillary",
    "session_shell",
    "session_tui_ops",
    "session_tui_side",
    "session_llm_oneshot",
    "session_connections",
)

def _pinned_commit() -> str:
    for line in support.PINNED_PATH.read_text(encoding="utf-8").splitlines():
        if line.startswith("commit="):
            return line.split("=", 1)[1]
    raise AssertionError("PINNED has no commit")


EXPECTED_OWNERS = [
    "from.comprehension",
    "from.group",
    "from.handlers",
    "from.methods",
    "from.subscript",
    "from.tuple",
    "profiles.list",
]


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def _synthetic_checkout(root: Path) -> None:
    gateway = root / "gateway"
    _write(
        gateway / "session_controls.py",
        "\n".join(
            [
                "class AuthorityConnection:",
                "    def handlers(self):",
                "        handlers = {'from.handlers': None}",
                "        handlers['from.subscript'] = None",
                "        unrelated = {'from.unrelated': None}",
                "        other = {}",
                "        other['from.other'] = None",
                "        return handlers",
                "",
                "    def dispatch(self, method):",
                "        table = {'profiles.list': self.handlers}",
                "        return table[method]",
                "",
            ]
        ),
    )
    bodies = {
        "session_busy_controls": (
            "def handlers():\n"
            "    source = {'from.comprehension': None}\n"
            "    return {name: source[name] for name in source}\n"
        ),
        "session_queued_controls": (
            "def handlers():\n"
            "    pairs = (('from.tuple', None),)\n"
            "    return {name: fn for name, fn in pairs}\n"
        ),
        "session_connections": (
            "METHODS = ('from.methods',)\n"
            "\n"
            "def handlers():\n"
            "    return {}\n"
        ),
    }
    for module in HANDLER_MODULES:
        _write(
            gateway / (module + ".py"),
            bodies.get(module, "def handlers():\n    return {}\n"),
        )
    _write(
        gateway / "session_group_controls.py",
        "GROUP_METHODS = {'from.group': None}\n",
    )


class NativeTests(unittest.TestCase):
    def test_canonical_methods_native_and_shared_names_only_through_canonical(self) -> None:
        contract = support.load_contract()
        canonical = [method["name"] for method in contract["x-canonical-methods"]]
        legacy = {method["name"] for method in contract["methods"]}
        shared = sorted(set(canonical) & legacy)
        self.assertIn("prompt.submit", shared)
        self.assertIn("clarify.lock", shared)
        for name in canonical:
            self.assertTrue(contract_gen.is_native("canonical", name, []))
        for name in shared:
            self.assertFalse(contract_gen.is_native("legacy", name, [name]))
            self.assertTrue(contract_gen.is_native("canonical", name, []))
        self.assertTrue(contract_gen.is_native("canonical", "runtime.describe", []))
        self.assertFalse(contract_gen.is_native("legacy", "session.compress", []))

    def test_fallback_rule_table(self) -> None:
        empty = []
        owners = ["process.stop", "reload.mcp"]
        self.assertFalse(contract_gen.is_native_legacy("session.compress", owners))
        self.assertFalse(contract_gen.is_native_legacy("agents.list", owners))
        self.assertFalse(contract_gen.is_native_legacy("agents.list", empty))
        self.assertTrue(contract_gen.is_native_legacy("session.most_recent", empty))
        self.assertTrue(contract_gen.is_native_legacy("pet.info", empty))
        self.assertFalse(contract_gen.is_native_legacy("process.stop", empty))
        self.assertFalse(contract_gen.is_native_legacy("reload.mcp", empty))
        self.assertTrue(contract_gen.is_native_legacy("process.stop", owners))
        self.assertTrue(contract_gen.is_native_legacy("reload.mcp", owners))

    def test_named_legacy_methods_are_not_native(self) -> None:
        contract = support.load_contract()
        got = list(contract_gen.non_native_legacy_names(contract))
        self.assertEqual(got, NOT_NATIVE)
        self.assertEqual(len(got), len(set(got)))

    def test_owner_list_extraction_from_a_synthetic_tree(self) -> None:
        commit = _pinned_commit()
        with support.temporary_directory() as temp_name:
            root = Path(temp_name)
            checkout = root / "checkout"
            _synthetic_checkout(checkout)
            contract_gen.require_pinned_checkout(checkout, commit)
            names = list(contract_gen.owner_methods_from_checkout(checkout))
            self.assertEqual(names, EXPECTED_OWNERS)
            self.assertNotIn("from.unrelated", names)
            self.assertNotIn("from.other", names)

            repo = root / "wrong-head"
            repo.mkdir()
            (repo / "README").write_text("fixture\n", encoding="utf-8")
            git_email = [
                "git",
                "-C",
                str(repo),
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.com",
            ]
            subprocess.run(["git", "-C", str(repo), "init"], check=True, capture_output=True)
            subprocess.run(git_email + ["add", "README"], check=True, capture_output=True)
            subprocess.run(
                git_email + ["commit", "-m", "fixture"],
                check=True,
                capture_output=True,
            )
            head = subprocess.run(
                ["git", "-C", str(repo), "rev-parse", "HEAD"],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
            self.assertNotEqual(head, commit)
            with self.assertRaises(contract_gen.CheckoutError) as caught:
                contract_gen.require_pinned_checkout(repo, commit)
            message = str(caught.exception)
            self.assertIn(commit, message)
            self.assertIn(head, message)
            self.assertNotIn("Traceback", message)


if __name__ == "__main__":
    unittest.main()
