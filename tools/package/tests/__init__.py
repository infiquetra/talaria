"""Discovery loads the packager cases in table order."""

from __future__ import annotations

import importlib
import unittest
from fnmatch import fnmatch


_ORDER = (
    ("test_targets.py", "TargetTests", "test_target_table_is_exactly_the_four_targets"),
    ("test_versions.py", "VersionTests", "test_tag_version_rules"),
    ("test_binary.py", "BinaryTests", "test_header_reader_identifies_arch_and_static"),
    ("test_binary.py", "BinaryTests", "test_can_run_on_host"),
    ("test_binary.py", "BinaryTests", "test_version_line_rules"),
    ("test_archive.py", "ArchiveTests", "test_archive_layout_modes_and_owners"),
    ("test_archive.py", "ArchiveTests", "test_archive_bytes_are_reproducible"),
    ("test_archive.py", "ArchiveTests", "test_archive_refuses_bad_inputs"),
    ("test_checksums.py", "ChecksumTests", "test_sums_format_and_order"),
    ("test_checksums.py", "ChecksumTests", "test_sums_refuse_wrong_archive_set"),
    ("test_checksums.py", "ChecksumTests", "test_plan_upload_never_replaces"),
    ("test_formula.py", "FormulaTests", "test_formula_matches_golden"),
    ("test_formula.py", "FormulaTests", "test_formula_refuses_bad_inputs"),
    ("test_formula.py", "FormulaTests", "test_formula_is_valid_ruby"),
    ("test_cli.py", "CliTests", "test_usage_errors_and_exit_codes"),
    ("test_cli.py", "CliTests", "test_build_failure_paths"),
    ("test_workflows.py", "WorkflowTests", "test_release_and_package_workflows_shape"),
    ("test_docs.py", "DocsTests", "test_install_page_contents"),
    ("test_live_build.py", "LiveBuildTests", "test_host_target_builds_packs_and_checks"),
)


def load_tests(loader, standard_tests, pattern):
    selected = pattern or "test*.py"
    suite = unittest.TestSuite()
    for filename, class_name, method_name in _ORDER:
        if not fnmatch(filename, selected):
            continue
        module_name = filename[:-3]
        module = importlib.import_module(f"tests.{module_name}")
        case_cls = getattr(module, class_name)
        suite.addTest(case_cls(method_name))
    return suite
