"""R20: the child sees only the default-deny environment plus the operator
allowlist. KTD5's five-variable TALARIA_* enumeration and the credential-shaped
deny are asserted directly against :func:`build_child_env`, not filtered from
a live child's actual environment (the doc's own "asserted, not filtered").
"""

from __future__ import annotations

from talaria.status.contract import (
    FORWARDED_TALARIA_VARS,
    build_child_env,
)


def _parent_env(**overrides: str) -> dict[str, str]:
    base = {
        "PATH": "/usr/bin:/bin",
        "HOME": "/home/op",
        "SHELL": "/bin/zsh",
        "TERM": "xterm-256color",
        "LANG": "en_US.UTF-8",
        "LC_ALL": "en_US.UTF-8",
        "TMPDIR": "/tmp",
    }
    base.update(overrides)
    return base


def test_base_passthrough_names_are_forwarded() -> None:
    env = build_child_env(parent_env=_parent_env())
    for name in ("PATH", "HOME", "SHELL", "TERM", "TMPDIR"):
        assert env[name] == _parent_env()[name]


def test_locale_variables_pass_through_by_prefix() -> None:
    env = build_child_env(parent_env=_parent_env(LC_TIME="fr_FR.UTF-8"))
    assert env["LANG"] == "en_US.UTF-8"
    assert env["LC_ALL"] == "en_US.UTF-8"
    assert env["LC_TIME"] == "fr_FR.UTF-8"


def test_canary_variable_in_talaria_env_is_absent_from_child() -> None:
    """A variable that exists in Talaria's own environment but is not part of
    KTD5's default-deny set, the five-variable TALARIA_* enumeration, or the
    operator allowlist must never reach the child."""
    env = build_child_env(
        parent_env=_parent_env(CANARY_SHOULD_NOT_LEAK="visible-if-this-leaks"),
        allowlist=(),
    )
    assert "CANARY_SHOULD_NOT_LEAK" not in env


def test_hermes_dashboard_session_token_never_passes_even_if_allowlisted() -> None:
    env = build_child_env(
        parent_env=_parent_env(HERMES_DASHBOARD_SESSION_TOKEN="s3cr3t"),
        allowlist=("HERMES_DASHBOARD_SESSION_TOKEN",),
    )
    assert "HERMES_DASHBOARD_SESSION_TOKEN" not in env


def test_only_the_five_enumerated_talaria_vars_are_candidates() -> None:
    assert set(FORWARDED_TALARIA_VARS) == {
        "TALARIA_CONFIG_DIR",
        "TALARIA_GATEWAY_URL",
        "TALARIA_PROFILE",
        "TALARIA_LOG_LEVEL",
        "TALARIA_STATUS_INTERVAL",
    }


def test_talaria_var_outside_the_enumeration_is_dropped() -> None:
    env = build_child_env(
        parent_env=_parent_env(TALARIA_SOME_FUTURE_FIELD="value"),
    )
    assert "TALARIA_SOME_FUTURE_FIELD" not in env


def test_talaria_credentials_var_is_dropped_by_name_despite_the_prefix() -> None:
    """KTD5: a TALARIA_* prefix is not by itself a pass — the credential-shaped
    deny outranks the enumeration and would catch a var like
    TALARIA_API_TOKEN if one ever existed, even though this exact name is
    outside FORWARDED_TALARIA_VARS anyway (belt and suspenders)."""
    env = build_child_env(parent_env=_parent_env(TALARIA_API_TOKEN="abc123"))
    assert "TALARIA_API_TOKEN" not in env


def test_enumerated_talaria_vars_forward_when_present() -> None:
    env = build_child_env(
        parent_env=_parent_env(
            TALARIA_CONFIG_DIR="/home/op/.talaria",
            TALARIA_PROFILE="default",
            TALARIA_LOG_LEVEL="info",
            TALARIA_STATUS_INTERVAL="10",
        )
    )
    assert env["TALARIA_CONFIG_DIR"] == "/home/op/.talaria"
    assert env["TALARIA_PROFILE"] == "default"
    assert env["TALARIA_LOG_LEVEL"] == "info"
    assert env["TALARIA_STATUS_INTERVAL"] == "10"


def test_gateway_url_query_string_is_stripped_entirely() -> None:
    env = build_child_env(
        parent_env=_parent_env(
            TALARIA_GATEWAY_URL="ws://127.0.0.1:9119/api/ws?token=s3cr3t&extra=1"
        )
    )
    assert env["TALARIA_GATEWAY_URL"] == "ws://127.0.0.1:9119/api/ws"
    assert "token" not in env["TALARIA_GATEWAY_URL"]
    assert "?" not in env["TALARIA_GATEWAY_URL"]


def test_gateway_url_fragment_is_also_stripped() -> None:
    env = build_child_env(
        parent_env=_parent_env(TALARIA_GATEWAY_URL="ws://127.0.0.1:9119/api/ws#frag")
    )
    assert env["TALARIA_GATEWAY_URL"] == "ws://127.0.0.1:9119/api/ws"


def test_operator_allowlist_forwards_named_variables() -> None:
    env = build_child_env(
        parent_env=_parent_env(MY_PROJECT_ENV="staging"),
        allowlist=("MY_PROJECT_ENV",),
    )
    assert env["MY_PROJECT_ENV"] == "staging"


def test_allowlist_cannot_rescue_a_credential_shaped_name() -> None:
    env = build_child_env(
        parent_env=_parent_env(MY_API_SECRET="s3cr3t"),
        allowlist=("MY_API_SECRET",),
    )
    assert "MY_API_SECRET" not in env


def test_allowlist_entry_absent_from_parent_env_is_a_noop() -> None:
    env = build_child_env(parent_env=_parent_env(), allowlist=("NEVER_SET",))
    assert "NEVER_SET" not in env


def test_child_env_has_no_terminal_framework_or_unexpected_keys() -> None:
    """A sanity bound: the constructed env is only ever base + locale +
    the enumerated TALARIA_* set + the allowlist — never a copy of the whole
    parent environment (KTD5: "Talaria's own environment never passes through
    wholesale")."""
    parent = _parent_env(
        TEXTUAL_DRIVER="headless",
        RANDOM_OTHER_VAR="x",
    )
    env = build_child_env(parent_env=parent, allowlist=())
    assert "TEXTUAL_DRIVER" not in env
    assert "RANDOM_OTHER_VAR" not in env


# ── Credential leaks found by adversarial review, 2026-08-03 ────────────


def test_a_lang_prefixed_name_is_not_mistaken_for_a_locale_variable() -> None:
    """``LANG`` matched as a prefix forwards LANGCHAIN_API_KEY by default.

    The credential-name deny list does not catch it either: ``is_suspicious_key``
    anchors its API-key pattern to the whole name.
    """
    child = build_child_env(
        parent_env={
            "LANG": "en_US.UTF-8",
            "LC_ALL": "en_US.UTF-8",
            "LANGCHAIN_API_KEY": "leak-1",
            "LANGSMITH_ENDPOINT": "https://leak-2",
        },
        allowlist=(),
    )
    assert child["LANG"] == "en_US.UTF-8"
    assert child["LC_ALL"] == "en_US.UTF-8"
    assert "LANGCHAIN_API_KEY" not in child
    assert "LANGSMITH_ENDPOINT" not in child


def test_gateway_url_userinfo_is_dropped_along_with_the_query() -> None:
    """A password in the URL is credential material even without a query."""
    child = build_child_env(
        parent_env={
            "TALARIA_GATEWAY_URL": "https://alice:s3cr3tpw@gw.example.com/api/ws?token=T",
        },
        allowlist=(),
    )
    forwarded = child["TALARIA_GATEWAY_URL"]
    assert forwarded == "https://gw.example.com/api/ws"
    assert "s3cr3tpw" not in forwarded
    assert "alice" not in forwarded
    assert "token" not in forwarded


def test_stripping_userinfo_keeps_the_port_and_ipv6_brackets() -> None:
    """The rebuild must not corrupt ordinary loopback or IPv6 gateway URLs."""
    child = build_child_env(
        parent_env={"TALARIA_GATEWAY_URL": "ws://127.0.0.1:8765/api/ws?token=T"},
        allowlist=(),
    )
    assert child["TALARIA_GATEWAY_URL"] == "ws://127.0.0.1:8765/api/ws"

    child6 = build_child_env(
        parent_env={"TALARIA_GATEWAY_URL": "ws://[::1]:8765/api/ws?token=T"},
        allowlist=(),
    )
    assert child6["TALARIA_GATEWAY_URL"] == "ws://[::1]:8765/api/ws"
