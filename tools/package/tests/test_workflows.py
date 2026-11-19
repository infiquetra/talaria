"""Text shape of the release and package workflows."""

from __future__ import annotations

import re
import unittest

from .support import repo_root


_FORBIDDEN = "un" + "attended"


def _run_lines(text):
    lines = text.splitlines()
    chosen = []
    index = 0
    while index < len(lines):
        line = lines[index]
        if re.search(r"run:\s*[|>]", line):
            indent = len(line) - len(line.lstrip(" "))
            index += 1
            while index < len(lines):
                current = lines[index]
                if current.strip() == "":
                    chosen.append(current)
                    index += 1
                    continue
                width = len(current) - len(current.lstrip(" "))
                if width <= indent:
                    break
                chosen.append(current)
                index += 1
            continue
        if re.search(r"run:\s*\S", line):
            chosen.append(line)
        index += 1
    return chosen


def _strip_yaml_comment(text):
    in_single = False
    in_double = False
    for index, char in enumerate(text):
        if char == "'" and not in_double:
            in_single = not in_single
        elif char == '"' and not in_single:
            in_double = not in_double
        elif char == "#" and not in_single and not in_double:
            return text[:index]
    return text


def _split_yaml_key(text):
    in_single = False
    in_double = False
    for index, char in enumerate(text):
        if char == "'" and not in_double:
            in_single = not in_single
        elif char == '"' and not in_single:
            in_double = not in_double
        elif char == ":" and not in_single and not in_double:
            rest = text[index + 1 :]
            if rest == "" or rest.startswith(" ") or rest.startswith("\t"):
                return text[:index].strip(), rest.strip()
    raise ValueError(f"expected a YAML key: {text!r}")


_ANCHOR_OR_ALIAS = re.compile(r"(^|\s)[&*](?![&*])[A-Za-z0-9_-]+")
_DOUBLE_ESCAPES = {
    "0": "\0",
    "a": "\a",
    "b": "\b",
    "t": "\t",
    "n": "\n",
    "v": "\v",
    "f": "\f",
    "r": "\r",
    "e": "\x1b",
    " ": " ",
    '"': '"',
    "/": "/",
    "\\": "\\",
    "N": "\u0085",
    "_": "\u00a0",
    "L": "\u2028",
    "P": "\u2029",
}


def _unquoted_pieces(token):
    pieces = []
    buffer = []
    index = 0
    in_single = False
    in_double = False
    while index < len(token):
        char = token[index]
        if in_double:
            if char == "\\":
                index += 2
                continue
            if char == '"':
                in_double = False
            index += 1
            continue
        if in_single:
            if char == "'" and index + 1 < len(token) and token[index + 1] == "'":
                index += 2
                continue
            if char == "'":
                in_single = False
            index += 1
            continue
        if char == '"':
            if buffer:
                pieces.append("".join(buffer))
                buffer = []
            in_double = True
            index += 1
            continue
        if char == "'":
            if buffer:
                pieces.append("".join(buffer))
                buffer = []
            in_single = True
            index += 1
            continue
        buffer.append(char)
        index += 1
    if in_single or in_double:
        raise ValueError(f"unclosed quoted YAML scalar: {token!r}")
    if buffer:
        pieces.append("".join(buffer))
    return pieces


def _reject_anchor_alias_or_merge(token, *, key):
    outside = " ".join(_unquoted_pieces(token))
    if _ANCHOR_OR_ALIAS.search(outside):
        raise ValueError(f"unsupported YAML anchor or alias: {token!r}")
    if key and outside.strip().startswith("<<"):
        raise ValueError(f"unsupported YAML merge key: {token!r}")


def _unescape_double_quoted(body):
    chosen = []
    index = 0
    while index < len(body):
        char = body[index]
        if char != "\\":
            chosen.append(char)
            index += 1
            continue
        if index + 1 >= len(body):
            raise ValueError("dangling escape in double-quoted YAML")
        marker = body[index + 1]
        if marker in _DOUBLE_ESCAPES:
            chosen.append(_DOUBLE_ESCAPES[marker])
            index += 2
            continue
        width = {"x": 2, "u": 4, "U": 8}.get(marker)
        if width is None:
            raise ValueError(f"unsupported YAML escape: \\{marker}")
        digits = body[index + 2 : index + 2 + width]
        if len(digits) != width or any(item not in "0123456789abcdefABCDEF" for item in digits):
            raise ValueError(f"unsupported YAML escape: \\{marker}{digits}")
        chosen.append(chr(int(digits, 16)))
        index += 2 + width
    return "".join(chosen)


def _parse_quoted_scalar(token):
    quote = token[:1]
    if quote not in {"'", '"'}:
        raise ValueError(f"expected a quoted YAML scalar: {token!r}")
    index = 1
    body = []
    while index < len(token):
        char = token[index]
        if quote == "'" and char == "'":
            if index + 1 < len(token) and token[index + 1] == "'":
                body.append("'")
                index += 2
                continue
            return "".join(body), index + 1
        if quote == '"' and char == "\\":
            if index + 1 >= len(token):
                raise ValueError(f"dangling escape in double-quoted YAML: {token!r}")
            body.append(char)
            body.append(token[index + 1])
            index += 2
            continue
        if quote == '"' and char == '"':
            return _unescape_double_quoted("".join(body)), index + 1
        body.append(char)
        index += 1
    raise ValueError(f"unclosed quoted YAML scalar: {token!r}")


def _parse_yaml_scalar(token):
    stripped = token.strip()
    if stripped[:1] in {"'", '"'}:
        value, end = _parse_quoted_scalar(stripped)
        if stripped[end:].strip():
            raise ValueError(f"unsupported YAML scalar syntax: {token!r}")
        return value
    if stripped[:1] in {"&", "*", "!", "{", "["} or stripped.startswith("<<"):
        raise ValueError(f"unsupported YAML syntax: {token!r}")
    return stripped


def _decode_mapping_key(token):
    _reject_anchor_alias_or_merge(token, key=True)
    stripped = token.strip()
    if stripped[:1] in {"'", '"'}:
        value, end = _parse_quoted_scalar(stripped)
        if stripped[end:].strip():
            raise ValueError(f"unsupported YAML key syntax: {token!r}")
        if value == "<<":
            raise ValueError(f"unsupported YAML merge key: {token!r}")
        if value == "":
            raise ValueError(f"empty YAML key: {token!r}")
        return value
    if stripped.startswith("<<"):
        raise ValueError(f"unsupported YAML merge key: {token!r}")
    if stripped[:1] in {"&", "*", "!", "{", "["}:
        raise ValueError(f"unsupported YAML key syntax: {token!r}")
    if stripped == "":
        raise ValueError(f"empty YAML key: {token!r}")
    return stripped


def _flow_items(text, start, end):
    items = []
    buffer = []
    depth = 0
    in_single = False
    in_double = False
    index = start
    while index < end:
        char = text[index]
        if char == "'" and not in_double:
            in_single = not in_single
            buffer.append(char)
        elif char == '"' and not in_single:
            in_double = not in_double
            buffer.append(char)
        elif not in_single and not in_double and char in "[{":
            depth += 1
            buffer.append(char)
        elif not in_single and not in_double and char in "]}":
            depth -= 1
            buffer.append(char)
        elif char == "," and depth == 0 and not in_single and not in_double:
            items.append("".join(buffer).strip())
            buffer = []
        else:
            buffer.append(char)
        index += 1
    tail = "".join(buffer).strip()
    if tail:
        items.append(tail)
    return items


def _parse_flow_sequence(token):
    if not token.startswith("["):
        raise ValueError(f"expected a flow sequence: {token!r}")
    closing = token.rfind("]")
    if closing < 1 or token[closing + 1 :].strip():
        raise ValueError(f"unclosed flow sequence: {token!r}")
    items = []
    for item in _flow_items(token, 1, closing):
        if not item:
            items.append(None)
            continue
        _reject_anchor_alias_or_merge(item, key=False)
        if item[:1] == "{":
            raise ValueError(f"unsupported YAML flow mapping: {item!r}")
        items.append(_parse_yaml_scalar(item))
    return items


def _is_block_scalar(token):
    if token[:1] not in {"|", ">"}:
        return False
    return token[1:] in {"", "-", "+"} or token[1:].isdigit()


def _grants_write(permissions):
    if permissions is None:
        return False
    if isinstance(permissions, str):
        return permissions in {"write", "write-all"}
    if isinstance(permissions, dict):
        return any(value == "write" for value in permissions.values())
    return True


class _WorkflowYaml:
    """Indentation parser for the workflow mapping these tests inspect."""

    def __init__(self, text):
        self.lines = text.splitlines()

    def parse(self):
        self.index = 0
        document = self._mapping(0)
        if self._peek()[1] is not None:
            line_number, _, line = self._peek()
            raise ValueError(f"trailing YAML at line {line_number + 1}: {line!r}")
        return document

    def _peek(self):
        index = self.index
        while index < len(self.lines):
            line = self.lines[index]
            if line.strip() == "" or line.lstrip().startswith("#"):
                index += 1
                continue
            indent = len(line) - len(line.lstrip(" "))
            if "\t" in line[:indent]:
                raise ValueError(f"tab indentation at line {index + 1}")
            return index, indent, line
        return None, None, None

    def _mapping(self, key_indent):
        mapping = {}
        while True:
            position, indent, line = self._peek()
            if position is None or indent != key_indent:
                break
            content = _strip_yaml_comment(line[indent:]).rstrip()
            if content.startswith("-"):
                break
            self.index = position + 1
            key, value = _split_yaml_key(content)
            mapping[_decode_mapping_key(key)] = self._value(key_indent, value)
        return mapping

    def _value(self, key_indent, token):
        if _is_block_scalar(token):
            return self._block(key_indent)
        if token == "":
            _, indent, line = self._peek()
            if indent is None or indent <= key_indent:
                return None
            if line.lstrip().startswith("-"):
                return self._sequence(indent)
            return self._mapping(indent)
        _reject_anchor_alias_or_merge(token, key=False)
        if token[:1] == "{":
            raise ValueError(f"unsupported YAML flow mapping: {token!r}")
        if token[:1] == "[":
            return _parse_flow_sequence(token)
        return _parse_yaml_scalar(token)

    def _sequence(self, dash_indent):
        items = []
        while True:
            position, indent, line = self._peek()
            if position is None or indent != dash_indent:
                break
            content = line[indent:]
            if not content.startswith("-"):
                break
            self.index = position + 1
            rest = content[1:].lstrip(" ") if content.startswith("- ") or content == "-" else None
            if rest is None:
                raise ValueError(f"malformed sequence item at line {position + 1}")
            rest = _strip_yaml_comment(rest).strip()
            if rest == "":
                _, nested_indent, nested_line = self._peek()
                if nested_indent is None or nested_indent <= dash_indent:
                    items.append(None)
                elif nested_line.lstrip().startswith("-"):
                    items.append(self._sequence(nested_indent))
                else:
                    items.append(self._mapping(nested_indent))
            elif rest[:1] == "{":
                raise ValueError(f"unsupported YAML flow mapping: {rest!r}")
            elif rest[:1] == "[":
                items.append(_parse_flow_sequence(rest))
            elif ":" in rest:
                key, value = _split_yaml_key(rest)
                items.append(self._mapping_from(dash_indent + 2, key, value))
            else:
                items.append(_parse_yaml_scalar(rest))
        return items

    def _mapping_from(self, key_indent, key, token):
        mapping = {_decode_mapping_key(key): self._value(key_indent, token)}
        mapping.update(self._mapping(key_indent))
        return mapping

    def _block(self, parent_indent):
        chosen = []
        content_indent = None
        while self.index < len(self.lines):
            line = self.lines[self.index]
            if line.strip() == "":
                if content_indent is None:
                    self.index += 1
                    continue
                chosen.append("")
                self.index += 1
                continue
            indent = len(line) - len(line.lstrip(" "))
            if content_indent is None:
                if indent <= parent_indent:
                    break
                content_indent = indent
            if indent < content_indent:
                break
            chosen.append(line[content_indent:])
            self.index += 1
        return "\n".join(chosen)


def _load_workflow_yaml(text):
    return _WorkflowYaml(text).parse()


class WorkflowTests(unittest.TestCase):
    def test_release_and_package_workflows_shape(self):
        root = repo_root() / ".github" / "workflows"
        release = (root / "release.yml").read_text(encoding="utf-8")
        build_path = root / "package-build.yml"
        check_path = root / "package-check.yml"
        self.assertTrue(build_path.is_file(), "jobs missing")
        self.assertTrue(check_path.is_file(), "jobs missing")
        build = build_path.read_text(encoding="utf-8")
        check = check_path.read_text(encoding="utf-8")
        for key in ("check:", "build:", "publish:", "tap:"):
            self.assertIn(f"\n  {key}\n", f"\n{release}", "jobs missing")

        self.assertIn("id: resolve", release, "jobs missing")
        self.assertIn(
            "=~ ^v[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.]+)?$",
            release,
            "jobs missing",
        )
        for step in (
            "Check out the tagged tree",
            "Install the pinned toolchain",
            "The tag agrees with the workspace version",
            "Project check",
        ):
            self.assertIn(step, release, "jobs missing")

        self.assertIn("--verify-tag", release, "jobs missing")
        for text in (release, build, check):
            self.assertNotIn("--clobber", text, "jobs missing")
            self.assertNotIn("cargo publish", text, "jobs missing")
            self.assertNotIn(_FORBIDDEN, text, "jobs missing")

        tap_at = release.index("\n  tap:\n")
        token = "secrets.HOMEBREW_TAP_TOKEN"
        self.assertEqual(release.find(token), release.rfind(token), "jobs missing")
        self.assertGreater(release.find(token), tap_at, "jobs missing")
        self.assertIn("if: needs.check.outputs.prerelease == 'false'", release, "jobs missing")
        self.assertIn('-z "$HOMEBREW_TAP_TOKEN"', release[tap_at:], "jobs missing")
        self.assertIn(
            "HOMEBREW_TAP_TOKEN is not set; the Homebrew tap was not updated",
            release[tap_at:],
            "jobs missing",
        )
        self.assertLess(
            release.find("gh pr list", tap_at),
            release.find("git commit", tap_at),
            "jobs missing",
        )

        for text in (release, build, check):
            for line in _run_lines(text):
                self.assertNotIn("${{ inputs.version }}", line, "jobs missing")
            for line in text.splitlines():
                stripped = line.strip()
                if not stripped.startswith("uses:") and " uses:" not in f" {stripped}":
                    if "uses:" not in stripped:
                        continue
                if "uses:" not in stripped:
                    continue
                if "./" in stripped:
                    continue
                self.assertRegex(stripped, r"@[0-9a-f]{40} #", "jobs missing")

        self.assertIn("package.py targets --json", build, "jobs missing")
        self.assertIn("fromJSON(needs.plan.outputs.matrix)", build, "jobs missing")
        check_at = release.index("\n  check:\n")
        build_at = release.index("\n  build:\n")
        self.assertIn("package.py version-gate", release[check_at:build_at], "jobs missing")
        self.assertIn("rustup toolchain install", check, "jobs missing")
        self.assertIn('PACKAGE_REQUIRE_RUBY: "1"', check, "jobs missing")

    def test_release_workflow_permissions_are_least_privilege(self):
        path = repo_root() / ".github" / "workflows" / "release.yml"
        document = _load_workflow_yaml(path.read_text(encoding="utf-8"))
        permissions = document["permissions"]
        jobs = document["jobs"]

        with self.subTest("workflow"):
            self.assertEqual(permissions, {"contents": "read"})
            self.assertNotIn("write", permissions.values())

        with self.subTest("publish"):
            self.assertEqual(
                jobs["publish"]["permissions"],
                {
                    "contents": "write",
                    "id-token": "write",
                    "attestations": "write",
                },
            )

        with self.subTest("tap"):
            self.assertEqual(jobs["tap"]["permissions"], {"contents": "read"})

        declared = {name for name, job in jobs.items() if "permissions" in job}
        with self.subTest("declared"):
            self.assertEqual(declared, {"publish", "tap"})

        writers = []
        for name, job in jobs.items():
            granted = job["permissions"] if "permissions" in job else permissions
            if _grants_write(granted):
                writers.append(name)
            if name == "publish":
                continue
            with self.subTest(job=name):
                self.assertFalse(_grants_write(granted), f"{name} grants a write scope")

        with self.subTest("writers"):
            self.assertEqual(writers, ["publish"])

    def test_quoted_permission_keys_cannot_hide_a_write_override(self):
        spellings = (
            '"permissions"',
            "'permissions'",
            '"permiss\\u0069ons"',
        )
        for spelling in spellings:
            text = (
                "permissions:\n"
                "  contents: read\n"
                "jobs:\n"
                "  check:\n"
                f"    {spelling}:\n"
                "      contents: write\n"
            )
            with self.subTest(spelling=spelling):
                document = _load_workflow_yaml(text)
                granted = document["jobs"]["check"]["permissions"]
                self.assertEqual(granted, {"contents": "write"})
                self.assertTrue(_grants_write(granted))

    def test_unsupported_workflow_yaml_fails_closed(self):
        samples = {
            "flow mapping": "jobs:\n  check:\n    permissions: {contents: write}\n",
            "merge key": "jobs:\n  check:\n    <<: *base\n",
            "anchor": "jobs:\n  check: &job\n    runs-on: ubuntu-latest\n",
            "alias": "jobs:\n  check:\n    permissions: *alias\n",
        }
        for name, text in samples.items():
            with self.subTest(name=name):
                with self.assertRaises(ValueError):
                    _load_workflow_yaml(text)
