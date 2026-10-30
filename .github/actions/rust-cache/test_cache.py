"""Offline cache routing and trust-boundary checks; no Cargo work or cache service."""

import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import cache

RELEASE_CACHE_IF = (
    "github.ref == format('refs/tags/{0}', steps.resolve.outputs.tag) && "
    "hashFiles('.github/actions/rust-cache/action.yml') != ''"
)


class CacheFixture(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "Cargo.lock").write_text("locked dependencies\n")
        (self.root / "Cargo.toml").write_text("[workspace]\n")
        (self.root / "rust-toolchain.toml").write_text(
            '[toolchain]\nchannel = "1.98.1"\n'
        )
        self.env = {
            "RUNNER_OS": "Linux",
            "RUNNER_ARCH": "X64",
            "GITHUB_EVENT_NAME": "push",
            "GITHUB_REF": "refs/heads/main",
            "DEFAULT_BRANCH": "main",
        }
        self.compiler = (
            "rustc 1.98.1\nrelease: 1.98.1\nhost: x86_64-unknown-linux-gnu\n"
        )
        self.cargo = "cargo 1.98.1"

    def plan(self, role: str = "native-debug", **changes: str) -> dict[str, str]:
        return cache.plan(
            self.root, role, self.env | changes, self.compiler, self.cargo
        )


class CachePolicyTests(CacheFixture):
    def test_identical_inputs_reuse_only_the_same_build_role(self) -> None:
        native = self.plan()
        self.assertEqual(native, self.plan())
        for role in cache.BUILD_PATHS:
            if role == "typify-install-release":
                continue
            with self.subTest(role=role):
                other = self.plan(role)
                self.assertEqual(native["download-key"], other["download-key"])
                if role != "native-debug":
                    self.assertNotEqual(native["build-key"], other["build-key"])
        self.assertEqual(
            self.plan("coverage-debug")["build-path"], "target/llvm-cov-target/debug"
        )
        self.assertEqual(self.plan("bench-release")["build-path"], "target/release")
        self.assertEqual(self.plan("contract-drift")["build-path"], "")

    def test_stale_platform_lock_toolchain_and_configuration_cannot_hit(self) -> None:
        original = self.plan()
        variants = [self.plan(RUNNER_OS="macOS"), self.plan(RUNNER_ARCH="ARM64")]
        variants.append(
            cache.plan(
                self.root,
                "native-debug",
                self.env,
                self.compiler + "commit-hash: changed\n",
                "cargo 1.98.1",
            )
        )
        variants.append(self.plan(RUSTFLAGS="-C target-cpu=native"))
        for name in ("Cargo.lock", "Cargo.toml", "rust-toolchain.toml"):
            path = self.root / name
            before = path.read_text()
            path.write_text(before + "# changed\n")
            variants.append(self.plan())
            path.write_text(before)
        for changed in variants:
            for key in ("download-key", "build-key"):
                self.assertNotEqual(original[key], changed[key])
                self.assertFalse(changed[key].startswith(original[key]))

    def test_missing_lock_wrong_compiler_and_unknown_role_disable_planning(
        self,
    ) -> None:
        with self.assertRaises(ValueError):
            self.plan("unknown")
        with self.assertRaises(ValueError):
            cache.plan(
                self.root,
                "native-debug",
                self.env,
                self.compiler.replace("1.98.1", "1.98.0"),
                "cargo 1.98.1",
            )
        (self.root / "Cargo.lock").unlink()
        with self.assertRaises(FileNotFoundError):
            self.plan()

    def test_only_default_branch_trusted_events_write_shared_caches(self) -> None:
        for event in ("push", "schedule", "workflow_dispatch"):
            self.assertEqual(
                self.plan(GITHUB_EVENT_NAME=event)["save-downloads"], "true"
            )
        for event in ("pull_request", "pull_request_target", "workflow_run"):
            for ref in ("refs/pull/42/merge", "refs/heads/main"):
                for role in cache.BUILD_PATHS:
                    if role == "typify-install-release":
                        continue
                    with self.subTest(event=event, ref=ref, role=role):
                        result = self.plan(
                            role, GITHUB_EVENT_NAME=event, GITHUB_REF=ref
                        )
                        self.assertEqual(result["save-downloads"], "false")
                        self.assertEqual(result["save-build"], "false")
        for ref in ("refs/heads/topic", "refs/tags/v1.2.3"):
            self.assertEqual(self.plan(GITHUB_REF=ref)["save-downloads"], "false")
        self.assertEqual(self.plan(DEFAULT_BRANCH="")["save-downloads"], "false")

    def test_release_saves_stay_within_trusted_tag(self) -> None:
        tagged = self.plan("release-debug", GITHUB_REF="refs/tags/v1.2.3-rc1")
        self.assertEqual(tagged["save-build"], "true")
        self.assertEqual(tagged["save-downloads"], "false")
        self.assertEqual(
            self.plan("native-debug", GITHUB_REF="refs/tags/v1.2.3")["save-build"],
            "false",
        )
        for ref in ("refs/tags/v1.2.3;bad", "refs/heads/topic", "refs/pull/42/merge"):
            self.assertEqual(
                self.plan("release-debug", GITHUB_REF=ref)["save-build"], "false"
            )
        self.assertNotEqual(
            tagged["build-key"], self.plan("coverage-debug")["build-key"]
        )
        for event in ("push", "schedule", "workflow_dispatch"):
            for ref in ("refs/heads/main", "refs/heads/topic"):
                result = self.plan(
                    "release-debug", GITHUB_EVENT_NAME=event, GITHUB_REF=ref
                )
                self.assertEqual(result["save-downloads"], "false")
                self.assertEqual(result["save-build"], "false")
        dispatched = self.plan(
            "release-debug",
            GITHUB_EVENT_NAME="workflow_dispatch",
            GITHUB_REF="refs/tags/v1.2.3",
        )
        self.assertEqual(dispatched["save-downloads"], "false")
        self.assertEqual(dispatched["save-build"], "false")


class TypifyCacheTests(CacheFixture):
    def setUp(self) -> None:
        super().setUp()
        self.home = self.root / "cargo-home"
        self.home.mkdir()
        self.env.update(
            {
                "CARGO_HOME": str(self.home),
                "HOME": str(self.root),
                "GITHUB_ACTIONS": "true",
                "RUNNER_ENVIRONMENT": "github-hosted",
                "ImageOS": "ubuntu24",
                "ImageVersion": "20261025.353.1",
            }
        )
        self.compiler += "commit-hash: " + "a" * 40 + "\n"
        self.cargo += (
            "\nhost: x86_64-unknown-linux-gnu\ncommit-hash: " + "b" * 40 + "\n"
        )

    def tool(self, **changes: str) -> dict[str, str]:
        return self.plan("typify-install-release", **changes)

    def test_fixed_tool_identity_and_image_are_bound_to_disjoint_keys(self) -> None:
        first = self.tool()
        self.assertEqual(first, self.tool())
        self.assertEqual(first["build-path"], "target/ci-tools/cargo-typify-0.8.0")
        self.assertEqual(first["save-downloads"], "false")
        self.assertEqual(first["save-build"], "true")
        self.assertLessEqual(len(first["build-key"]), 512)
        for field in (
            "package",
            "archive",
            "version",
            "source",
            "crate-sha256",
            "lock-sha256",
            "features",
            "profile",
            "target",
        ):
            with (
                self.subTest(field=field),
                patch.dict(cache.TYPIFY_IDENTITY, {field: "different"}),
            ):
                changed = self.tool()["build-key"]
                self.assertNotEqual(first["build-key"], changed)
                self.assertFalse(changed.startswith(first["build-key"]))
        for changes in (
            {"ImageVersion": "20261026.1.0"},
            {"ImageOS": "ubuntu26"},
            {"RUNNER_ARCH": "ARM64"},
            {"RUNNER_OS": "macOS"},
            {"RUSTFLAGS": "-C opt-level=2"},
            {"CARGO_PROFILE_RELEASE_OPT_LEVEL": "2"},
        ):
            changed = self.tool(**changes)["build-key"]
            self.assertNotEqual(first["build-key"], changed)
            self.assertFalse(changed.startswith(first["build-key"]))
        for role in cache.BUILD_PATHS:
            if role != "typify-install-release":
                self.assertNotEqual(first["build-key"], self.plan(role)["build-key"])

    def test_tool_writes_require_trusted_default_branch_without_tag_exception(
        self,
    ) -> None:
        for event in ("push", "schedule", "workflow_dispatch"):
            self.assertEqual(self.tool(GITHUB_EVENT_NAME=event)["save-build"], "true")
        for event in (
            "pull_request",
            "pull_request_target",
            "workflow_run",
            "unknown",
            "",
        ):
            for ref in ("refs/heads/main", "refs/pull/12/merge", "refs/tags/v1.2.3"):
                self.assertEqual(
                    self.tool(GITHUB_EVENT_NAME=event, GITHUB_REF=ref)["save-build"],
                    "false",
                )
        for ref in ("refs/heads/topic", "refs/tags/v1.2.3", ""):
            self.assertEqual(self.tool(GITHUB_REF=ref)["save-build"], "false")
        self.assertEqual(self.tool(DEFAULT_BRANCH="")["save-build"], "false")

    def test_compiler_cargo_pin_lock_and_flags_mutations_do_not_reuse_tool_objects(
        self,
    ) -> None:
        original = self.tool()["build-key"]
        variants = [
            cache.plan(
                self.root,
                cache.TYPIFY_ROLE,
                self.env,
                self.compiler.replace("a" * 40, "c" * 40),
                self.cargo,
            ),
            cache.plan(
                self.root,
                cache.TYPIFY_ROLE,
                self.env,
                self.compiler,
                self.cargo.replace("b" * 40, "d" * 40),
            ),
            self.tool(RUSTDOCFLAGS="--cfg cache_probe"),
            self.tool(CARGO_ENCODED_RUSTFLAGS="-C\x1fopt-level=2"),
            self.tool(CARGO_BUILD_RUSTFLAGS="-C opt-level=2"),
        ]
        for name in ("Cargo.lock", "Cargo.toml", "rust-toolchain.toml"):
            path = self.root / name
            before = path.read_text()
            path.write_text(before + "# changed\n")
            variants.append(self.tool())
            path.write_text(before)
        for changed in variants:
            self.assertNotEqual(original, changed["build-key"])
            self.assertFalse(changed["build-key"].startswith(original))
        for compiler, cargo in (
            ("release: 1.98.1", self.cargo),
            (self.compiler, "cargo 1.98.1"),
        ):
            with self.assertRaises(ValueError):
                cache.plan(self.root, cache.TYPIFY_ROLE, self.env, compiler, cargo)
        with self.assertRaises(ValueError):
            self.tool(RUNNER_OS="x" * 512)

    def test_environment_selection_checks_unsupported_names_without_reading_values(
        self,
    ) -> None:
        class PresenceOnly(dict):
            def __getitem__(self, name):
                if name in {"CARGO_REGISTRY_TOKEN", "GH_TOKEN", "CC"}:
                    raise AssertionError("credential or unsupported value was read")
                return super().__getitem__(name)

        source = PresenceOnly(
            self.env
            | {"CARGO_REGISTRY_TOKEN": "unused", "GH_TOKEN": "unused", "CC": "unused"}
        )
        selected = cache.selected_environment(cache.TYPIFY_ROLE, source)
        self.assertNotIn("GH_TOKEN", selected)
        self.assertEqual(selected["CARGO_REGISTRY_TOKEN"], "present")
        self.assertEqual(selected["CC"], "present")
        with self.assertRaises(ValueError):
            cache.plan(
                self.root, cache.TYPIFY_ROLE, selected, self.compiler, self.cargo
            )

    def test_unsupported_metadata_and_overrides_disable_optional_tool_planning(
        self,
    ) -> None:
        for name in ("ImageOS", "ImageVersion", "GITHUB_ACTIONS", "RUNNER_ENVIRONMENT"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.tool(**{name: ""})
        for name in (
            "RUSTC",
            "RUSTDOC",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "RUSTUP_TOOLCHAIN",
            "CARGO_BUILD_TARGET",
            "CARGO_TARGET_DIR",
            "CARGO_BUILD_BUILD_DIR",
            "CARGO_BUILD_RUSTC",
            "CARGO_INSTALL_ROOT",
            "CARGO_REGISTRIES_CRATES_IO_INDEX",
            "CARGO_REGISTRY_TOKEN",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER",
            "CC",
            "HOST_CC",
            "CXXFLAGS",
            "CC_x86_64_unknown_linux_gnu",
            "PKG_CONFIG_PATH",
            "CMAKE_TOOLCHAIN_FILE",
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "CARGO_PROFILE_RELEASE_CODEGEN_BACKEND",
            "LLVM_CONFIG_PATH",
        ):
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.tool(**{name: ""})
        for flags in (
            "-C linker=/other/cc",
            "--target wasm32-unknown-unknown",
            "-C target-cpu=native",
            "@/other/flags",
            "-L native=/other/lib",
            "-C target-feature=+avx2",
            "-C llvm-args=-load=/other/plugin",
        ):
            with self.subTest(flags=flags), self.assertRaises(ValueError):
                self.tool(RUSTFLAGS=flags)

    def test_home_config_presence_and_unsafe_paths_are_rejected_without_reading(
        self,
    ) -> None:
        for name in ("config", "config.toml"):
            config = self.home / name
            config.write_text("must not read this config")
            with (
                patch.object(
                    Path, "read_bytes", side_effect=AssertionError("read config")
                ),
                patch.object(
                    Path, "read_text", side_effect=AssertionError("read config")
                ),
                self.assertRaises(ValueError),
            ):
                self.tool()
            config.unlink()
        target = self.root / "target"
        target.symlink_to(self.home, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.tool()
        target.unlink()
        for path in (
            str(self.root),
            str(self.root / "target/ci-tools/cargo-typify-0.8.0/bin"),
            "relative",
        ):
            with self.subTest(path=path), self.assertRaises(ValueError):
                self.tool(CARGO_HOME=path)
        with (
            patch.dict(cache.BUILD_PATHS, {"native-debug": "target/ci-tools"}),
            self.assertRaises(ValueError),
        ):
            self.tool()
        config = self.home / "config.toml"
        config.symlink_to(self.root / "missing-config")
        with self.assertRaises(ValueError):
            self.tool()
        config.unlink()
        (self.root / ".cargo").mkdir()
        (self.root / ".cargo/config.toml").touch()
        with self.assertRaises(ValueError):
            self.tool()
        (self.root / ".cargo/config.toml").unlink()
        with (
            patch.dict(cache.BUILD_PATHS, {cache.TYPIFY_ROLE: "target/debug"}),
            self.assertRaises(ValueError),
        ):
            self.tool()
        (self.root / "target").mkdir()
        (self.root / "target/release").symlink_to(self.root / cache.TYPIFY_PATH)
        with self.assertRaises(ValueError):
            self.tool()
        (self.root / "target/release").unlink()
        home_file = self.root / "not-a-directory"
        home_file.touch()
        with self.assertRaises(ValueError):
            self.tool(CARGO_HOME=str(home_file))
        with self.assertRaises(ValueError):
            self.tool(CARGO_HOME=str(self.root / "line\nbreak"))
        with (
            patch.object(Path, "lstat", side_effect=PermissionError),
            self.assertRaises(ValueError),
        ):
            self.tool()


class WorkflowPolicyTests(unittest.TestCase):
    @staticmethod
    def field(block: str, key: str, indent: int) -> str:
        values = re.findall(
            r"(?m)^" + " " * indent + re.escape(key) + r": ([^\n]+)$", block
        )
        if len(values) != 1:
            raise ValueError(f"missing or duplicate cache field: {key}")
        return values[0]

    @classmethod
    def job_steps(cls, source: str) -> dict[str, list[str]]:
        # Check the existing, deliberately small workflow source shape without a YAML dependency.
        parts = re.split(r"(?m)^  ([\w-]+):[ \t]*$", source.split("\njobs:\n", 1)[1])
        jobs = dict(zip(parts[1::2], parts[2::2]))
        if len(jobs) != len(parts[1::2]):
            raise ValueError("duplicate workflow job")
        return {
            name: re.split(r"(?m)^      - ", body)[1:] for name, body in jobs.items()
        }

    @classmethod
    def check_job_bindings(
        cls, source: str, expected: dict[str, list[tuple[str, str]]]
    ) -> None:
        jobs = cls.job_steps(source)
        if jobs.keys() != expected.keys():
            raise ValueError("workflow jobs differ from the cache policy")
        for job, inventory in expected.items():
            caches = [
                block
                for block in jobs[job]
                if re.search(
                    r"(?m)^        uses: \./\.github/actions/rust-cache$", block
                )
            ]
            actual = [
                (
                    cls.field(block, "role", 10),
                    cache.BUILD_PATHS.get(cls.field(block, "role", 10)),
                )
                for block in caches
            ]
            if actual != inventory:
                raise ValueError(f"wrong ordered cache role/path inventory for {job}")
            if job == "release" and cls.field(caches[0], "if", 8) != RELEASE_CACHE_IF:
                raise ValueError(
                    "release cache must bind the event ref to the checked-out tag"
                )

    @classmethod
    def check_typify_installs(cls, source: str) -> list[str]:
        installs = []
        remaining = source
        directory = "${{ github.workspace }}/target/ci-tools/cargo-typify-0.8.0"
        for job, blocks in cls.job_steps(source).items():
            matches = [
                index
                for index, block in enumerate(blocks)
                if block.startswith("name: Install cargo-typify\n")
            ]
            if len(matches) != 1:
                raise ValueError(f"missing or duplicate typify installation in {job}")
            index = matches[0]
            block = blocks[index]
            if (
                cls.field(block, "run", 8)
                != "cargo install cargo-typify --version 0.8.0 --locked"
            ):
                raise ValueError("typify install command changed")
            if re.findall(r"(?m)^        ([\w-]+):", block) != ["env", "run"]:
                raise ValueError(
                    "typify installation must remain mandatory and fail on error"
                )
            if re.findall(r"(?m)^          ([\w-]+):", block) != [
                "CARGO_TARGET_DIR",
                "CARGO_BUILD_BUILD_DIR",
            ]:
                raise ValueError("unexpected typify installation environment")
            for name in ("CARGO_TARGET_DIR", "CARGO_BUILD_BUILD_DIR"):
                if cls.field(block, name, 10) != directory:
                    raise ValueError(
                        "typify compilation directories disagree with fixed policy"
                    )
            if (
                index == 0
                or cls.field(blocks[index - 1], "role", 10) != "typify-install-release"
            ):
                raise ValueError(
                    "tool cache must immediately precede mandatory installation"
                )
            installs.append(block)
            remaining = remaining.replace(block, "", 1)
        if len(installs) != 2 or source.count("cargo install cargo-typify") != 2:
            raise ValueError("exactly two original typify installs are required")
        if "CARGO_TARGET_DIR" in remaining or "CARGO_BUILD_BUILD_DIR" in remaining:
            raise ValueError("install-only directories leaked into another scope")
        return installs

    def test_source_guard_rejects_broadened_writer_authority(self) -> None:
        source = (Path(__file__).parent / "action.yml").read_text()
        for authority in ("save-downloads", "save-build"):
            condition = f"steps.plan.outputs.{authority} == 'true'"
            with self.subTest(authority=authority), self.assertRaises(ValueError):
                self.check_cache_steps(
                    source.replace(condition, condition + " || true", 1),
                    4,
                    composite=True,
                )

    def test_release_cache_requires_bound_tag_ref_and_existing_helper(self) -> None:
        root = Path(__file__).resolve().parents[3]
        source = (root / ".github/workflows/release.yml").read_text()
        expected = {"release": [("release-debug", "target/debug")]}
        for weakened in (
            "hashFiles('.github/actions/rust-cache/action.yml') != ''",
            "github.ref == format('refs/tags/{0}', steps.resolve.outputs.tag)",
            RELEASE_CACHE_IF + " || true",
        ):
            with self.subTest(condition=weakened), self.assertRaises(ValueError):
                self.check_job_bindings(
                    source.replace(RELEASE_CACHE_IF, weakened), expected
                )

    def test_each_job_role_and_path_binding_is_checked(self) -> None:
        root = Path(__file__).resolve().parents[3]
        source = (root / ".github/workflows/ci.yml").read_text()
        swapped = (
            source.replace("role: native-debug", "role: temporary")
            .replace("role: coverage-debug", "role: native-debug")
            .replace("role: temporary", "role: coverage-debug")
        )
        expected = {
            "rust-check": [("native-debug", "target/debug")],
            "rust-coverage": [("coverage-debug", "target/llvm-cov-target/debug")],
        }
        with self.assertRaises(ValueError):
            self.check_job_bindings(swapped, expected)
        original = cache.BUILD_PATHS["coverage-debug"]
        try:
            cache.BUILD_PATHS["coverage-debug"] = "target/debug"
            with self.assertRaises(ValueError):
                self.check_job_bindings(source, expected)
        finally:
            cache.BUILD_PATHS["coverage-debug"] = original

    @classmethod
    def check_cache_steps(
        cls, source: str, indent: int, composite: bool = False
    ) -> None:
        blocks = re.split(r"(?m)^" + " " * indent + r"- ", source)[1:]
        uses = 0
        inventory = []
        for block in blocks:
            if (
                "uses: actions/cache" not in block
                and "uses: ./.github/actions/rust-cache" not in block
            ):
                continue
            uses += 1
            if cls.field(block, "continue-on-error", indent + 2) != "true":
                raise ValueError("cache failure could block original work")
            if composite:
                if re.findall(r"(?m)^        ([\w-]+):", block) != ["key", "path"]:
                    raise ValueError("unexpected cache input or restore prefix")
                key = cls.field(block, "key", indent + 4)
                if key == "${{ steps.plan.outputs.download-key }}":
                    authority, prefix = (
                        "save-downloads",
                        "inputs.role != 'typify-install-release' && ",
                    )
                    paths = re.findall(r"(?m)^          (.+)$", block)
                    if cls.field(block, "path", 8) != "|" or paths != [
                        "${{ steps.plan.outputs.cargo-home }}/registry/index",
                        "${{ steps.plan.outputs.cargo-home }}/registry/cache",
                        "${{ steps.plan.outputs.cargo-home }}/registry/src",
                        "${{ steps.plan.outputs.cargo-home }}/git/db",
                    ]:
                        raise ValueError(
                            "download cache includes unsupported or installed state"
                        )
                elif key == "${{ steps.plan.outputs.build-key }}":
                    authority, prefix = (
                        "save-build",
                        "steps.plan.outputs.build-path != '' && ",
                    )
                    if (
                        cls.field(block, "path", indent + 4)
                        != "${{ steps.plan.outputs.build-path }}"
                    ):
                        raise ValueError(
                            "build cache does not use the planned role path"
                        )
                else:
                    raise ValueError("unknown cache key")
                action = cls.field(block, "uses", indent + 2)
                if (
                    action
                    == "actions/cache/restore@0057852bfaa89a56745cba8c7296529d2fc39830 # v4.3.0"
                ):
                    operator = "!="
                elif (
                    action
                    == "actions/cache@0057852bfaa89a56745cba8c7296529d2fc39830 # v4.3.0"
                ):
                    operator = "=="
                else:
                    raise ValueError("unexpected cache action")
                expected = prefix + f"steps.plan.outputs.{authority} {operator} 'true'"
                if cls.field(block, "if", indent + 2) != expected:
                    raise ValueError(
                        "cache writer or reader has lost its authority guard"
                    )
                inventory.append((authority, operator))
        if not uses:
            raise ValueError("missing cache integration")
        if composite and inventory != [
            ("save-downloads", "=="),
            ("save-downloads", "!="),
            ("save-build", "=="),
            ("save-build", "!="),
        ]:
            raise ValueError("unexpected ordered composite cache inventory")

    def test_all_roles_use_optional_caches_without_gating_original_work(self) -> None:
        root = Path(__file__).resolve().parents[3]
        roles = {
            "ci.yml": {
                "rust-check": [("native-debug", "target/debug")],
                "rust-coverage": [("coverage-debug", "target/llvm-cov-target/debug")],
            },
            "bench.yml": {"bench": [("bench-release", "target/release")]},
            "contract.yml": {
                "contract-gen-check": [
                    ("contract-check-debug", "target/debug"),
                    ("typify-install-release", "target/ci-tools/cargo-typify-0.8.0"),
                ],
                "contract-drift": [
                    ("contract-drift", ""),
                    ("typify-install-release", "target/ci-tools/cargo-typify-0.8.0"),
                ],
            },
            "release.yml": {"release": [("release-debug", "target/debug")]},
        }
        for name, expected in roles.items():
            source = (root / ".github/workflows" / name).read_text()
            with self.subTest(workflow=name):
                self.check_cache_steps(source, 6)
                self.check_job_bindings(source, expected)
                self.assertNotIn("cache-hit", source)
                if name == "contract.yml":
                    self.check_typify_installs(source)
        action = (Path(__file__).parent / "action.yml").read_text()
        self.check_cache_steps(action, 4, composite=True)
        self.assertNotIn("restore-keys:", action)
        self.assertNotIn("fail-on-cache-miss:", action)
        self.assertNotIn("/.cargo/bin", action)
        self.assertNotIn("target/criterion", action)
        self.assertNotIn("save-always:", action)

    def test_source_guards_reject_fail_closed_cache_steps_and_unguarded_writers(
        self,
    ) -> None:
        source = (Path(__file__).parent / "action.yml").read_text()
        for changed in (
            source.replace("continue-on-error: true", "continue-on-error: false", 1),
            source.replace("steps.plan.outputs.save-downloads == 'true'", "true", 1),
            source.replace("steps.plan.outputs.save-build == 'true'", "true", 1),
        ):
            with self.assertRaises(ValueError):
                self.check_cache_steps(changed, 4, composite=True)
        with self.assertRaises(ValueError):
            self.check_cache_steps("steps:\n  - run: cargo test --workspace\n", 2)

    def test_install_and_composite_mutations_cannot_bypass_the_fixed_contract(
        self,
    ) -> None:
        root = Path(__file__).resolve().parents[3]
        source = (root / ".github/workflows/contract.yml").read_text()
        for changed in (
            source.replace(" --locked", "", 1),
            source.replace("--version 0.8.0", "--version 0.9.0", 1),
            source.replace(
                "      - name: Install cargo-typify\n",
                "      - name: Install cargo-typify\n        if: steps.cache.outputs.cache-hit != 'true'\n",
                1,
            ),
            source.replace(
                "      - name: Install cargo-typify\n",
                "      - name: Install cargo-typify\n        continue-on-error: true\n",
                1,
            ),
            source.replace("--locked", "--locked || true", 1),
            source.replace(
                "CARGO_BUILD_BUILD_DIR: ${{ github.workspace }}/target/ci-tools/cargo-typify-0.8.0",
                "CARGO_BUILD_BUILD_DIR: ${{ github.workspace }}/target/release",
                1,
            ),
            source.replace("jobs:\n", "env:\n  CARGO_TARGET_DIR: /other\njobs:\n", 1),
        ):
            with self.assertRaises(ValueError):
                self.check_typify_installs(changed)
        blocks = self.job_steps(source)
        expected = {
            "contract-gen-check": [
                ("contract-check-debug", "target/debug"),
                ("typify-install-release", "target/ci-tools/cargo-typify-0.8.0"),
            ],
            "contract-drift": [
                ("contract-drift", ""),
                ("typify-install-release", "target/ci-tools/cargo-typify-0.8.0"),
            ],
        }
        call = next(
            block
            for block in blocks["contract-gen-check"]
            if "role: typify-install-release" in block
        )
        for changed in (
            source.replace("      - " + call, "      - " + call + "      - " + call, 1),
            source.replace(
                "role: contract-check-debug", "role: typify-install-release", 1
            ),
        ):
            with self.assertRaises(ValueError):
                self.check_job_bindings(changed, expected)
        action = (Path(__file__).parent / "action.yml").read_text()
        for changed in (
            action.replace("/registry/src", "/bin", 1),
            action.replace("/registry/src", "/.crates2.json", 1),
            action.replace(
                "        path:", "        restore-keys: cargo-\n        path:", 1
            ),
            action.replace("inputs.role != 'typify-install-release' && ", "", 1),
            action.replace(
                "${{ steps.plan.outputs.build-path }}",
                "${{ steps.plan.outputs.cargo-home }}/bin",
                1,
            ),
        ):
            with self.assertRaises(ValueError):
                self.check_cache_steps(changed, 4, composite=True)

    def test_actual_install_command_boundary_still_runs_after_cache_miss_hit_or_error(
        self,
    ) -> None:
        root = Path(__file__).resolve().parents[3]
        source = (root / ".github/workflows/contract.yml").read_text()
        installs = self.check_typify_installs(source)
        with tempfile.TemporaryDirectory() as name:
            workspace = Path(name)
            cargo = workspace / "cargo"
            log = workspace / "install.jsonl"
            cargo.write_text(
                f"#!{sys.executable}\n"
                + (
                    "import json, os, sys\n"
                    "with open(os.environ['INSTALL_LOG'], 'a') as stream:\n"
                    "    stream.write(json.dumps({'argv': sys.argv[1:], 'target': os.environ['CARGO_TARGET_DIR'], 'build': os.environ['CARGO_BUILD_BUILD_DIR']}) + '\\n')\n"
                    "sys.exit(int(os.environ['INSTALL_EXIT']))\n"
                )
            )
            cargo.chmod(0o755)
            for install in installs:
                run = self.field(install, "run", 8)
                env = {
                    "PATH": str(workspace),
                    "INSTALL_LOG": str(log),
                    "HOME": str(workspace),
                    "GITHUB_OUTPUT": str(workspace / "outputs"),
                    **{
                        key: self.field(install, key, 10).replace(
                            "${{ github.workspace }}", str(workspace)
                        )
                        for key in ("CARGO_TARGET_DIR", "CARGO_BUILD_BUILD_DIR")
                    },
                }
                for state in ("miss", "hit", "error", "unsupported"):
                    # The service is stubbed; unsupported metadata exercises the real planner CLI.
                    if state == "unsupported":
                        optional = subprocess.run(
                            [
                                sys.executable,
                                "-I",
                                str(root / ".github/actions/rust-cache/cache.py"),
                                "typify-install-release",
                            ],
                            env=env,
                            check=False,
                            capture_output=True,
                        )
                    else:
                        optional = subprocess.run(
                            [
                                sys.executable,
                                "-c",
                                "import sys; sys.exit("
                                + ("1" if state == "error" else "0")
                                + ")",
                            ],
                            check=False,
                            capture_output=True,
                        )
                    self.assertEqual(
                        optional.returncode != 0, state in {"error", "unsupported"}
                    )
                    for exit_code in (0, 23):
                        result = subprocess.run(
                            ["/bin/bash", "-e", "-o", "pipefail", "-c", run],
                            env=env | {"INSTALL_EXIT": str(exit_code)},
                            check=False,
                            capture_output=True,
                        )
                        self.assertEqual(result.returncode, exit_code)
            records = [json.loads(line) for line in log.read_text().splitlines()]
            self.assertEqual(len(records), 16)
            for record in records:
                self.assertEqual(
                    record["argv"],
                    ["install", "cargo-typify", "--version", "0.8.0", "--locked"],
                )
                self.assertEqual(record["target"], record["build"])
                self.assertEqual(
                    record["target"],
                    str(workspace / "target/ci-tools/cargo-typify-0.8.0"),
                )


if __name__ == "__main__":
    unittest.main()
