"""Staging files and images to the gateway, as outcomes (C9).

Talaria is a remote client: it never owns the gateway process, so the file
it stages lives only on the operator's disk. That single fact selects the
upload variants of the gateway surface (corrected contract on #147):
``file.attach`` carries ``{session_id, data_url, name}`` — notably *no*
client path, which would ask the gateway to read a disk it cannot see —
and ``image.attach_bytes`` carries ``{session_id, content_base64}``. The
gateway stages the bytes and answers with a reference; the two paths then
diverge and this module does not merge them back together.

**No route lives here.** The operator picks a typed path or a dropped path
(D6, decided); both resolve to a path first, and from a path down the
contract is identical. Nothing here names a route. One deliberate
asymmetry: portable-document format is out of scope for v0.6.1, and
``file.attach`` *would* accept a PDF, so :func:`attach_file` refuses PDFs
itself (extension and magic bytes) — a Talaria-side refusal, never sent.
Images carry no such refusal: the gateway sniffs magic bytes and answers
4016 itself, which is exactly the real unsupported-type case. There is no
``pdf.attach`` and none is invented here.

**The client places the reference; the gateway does not inject it.** A
successful :func:`attach_file` returns the ``@file:`` ref verbatim in
``prompt_text``: the route layer must put that string into the composer
text ``prompt.submit`` sends, because on the turn the gateway inlines the
UTF-8 content under ``--- Attached Context ---``. For images
``prompt_text`` is ``None`` on purpose: no ``@file:`` reference goes in
the prompt — ``prompt.submit`` drains the staged-image queue itself,
sending native parts or a vision instruction. ``prompt.submit`` takes
text-side params only and is untouched: attachments ride session state.

**Failures are outcomes, classified once.** Gateway codes map to
client-side kinds with honest retry guidance in
:func:`classify_gateway_error`. An ``unknown`` RPC outcome stays unknown:
the gateway may have staged the file and lost the reply, so retrying may
create a second staged copy — the detail says exactly that instead of
calling the retry safe.

**The gateway reference proves staging, never delivery.** Both attach
functions mean the agent *can* read the content, not that it has. Callers
must not report either as delivered.

Corrected contract on #147; parameter names corroborated against
``tui_gateway/server.py`` (Hermes ``8980b816f``).
"""

from __future__ import annotations

import base64
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Final, Literal, Protocol

from talaria.transport.rpc import RpcOutcome

__all__ = [
    "ATTACH_TIMEOUT_SECONDS",
    "AttachmentDispatcher",
    "AttachmentErrorKind",
    "AttachmentOutcome",
    "AttachmentStatus",
    "DetachOutcome",
    "FileAttachOutcome",
    "ImageAttachOutcome",
    "attach_file",
    "attach_image_file",
    "build_data_url",
    "classify_gateway_error",
    "detach_image",
    "mime_for_filename",
]

#: Default deadline for one attach call. Staging is a single small upload;
#: PDF rendering happens gateway-side on later reads, not inside this call.
ATTACH_TIMEOUT_SECONDS: Final[float] = 30.0

#: Outcome of one staging attempt. ``unknown`` is first-class: an
#: interrupted call may still have staged the file gateway-side.
AttachmentStatus = Literal["ok", "error", "unknown"]

#: Client-side failure kinds. ``attached``/``detached`` are the success
#: kinds, so a kind alone always says what happened without re-reading
#: ``status``.
AttachmentErrorKind = Literal[
    "attached",
    "detached",
    "missing-file",
    "unreadable-file",
    "invalid-request",
    "unsupported-type",
    "bad-payload",
    "too-large",
    "staging-failed",
    "gateway-error",
    "unknown",
]


class AttachmentDispatcher(Protocol):
    """The one operation attachment staging needs.

    Declared here rather than imported (cf.
    :class:`~talaria.transport.compat_check.ProbeDispatcher`): transport
    modules name callable shapes locally so nothing reaches sideways for
    them. :meth:`LiveSource.call <talaria.transport.source.LiveSource.call>`
    satisfies this protocol.
    """

    async def call(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        *,
        timeout: float | None = None,
    ) -> RpcOutcome: ...


@dataclass(frozen=True)
class AttachmentOutcome:
    """What is known about one staging attempt. Never claims more."""

    status: AttachmentStatus
    kind: AttachmentErrorKind
    #: Human-readable detail, safe to show the operator. Carries the
    #: gateway's own message plus what to do next — never a bare code.
    detail: str
    method: str
    #: The exact string the route layer must place into the composer text
    #: ``prompt.submit`` sends (the ``@file:`` ref for files). ``None``
    #: means nothing goes in the prompt — images drain from the staged
    #: queue on submit, and failures and unknowns insert nothing.
    prompt_text: str | None = None
    #: The gateway-issued reference (``@file:`` ref or staged image path).
    #: ``None`` unless the gateway confirmed staging. Safe to record.
    gateway_ref: str | None = None
    #: Bytes the gateway confirmed it staged, when it says so.
    bytes_staged: int | None = None


@dataclass(frozen=True)
class FileAttachOutcome(AttachmentOutcome):
    """A ``file.attach`` outcome, with the staged file's gateway names."""

    #: File name as staged gateway-side. Safe to record.
    name: str = ""
    #: Whether the gateway materialized uploaded bytes (``True`` for the
    #: remote-client case) or referenced a file already in its workspace.
    uploaded: bool = False


@dataclass(frozen=True)
class ImageAttachOutcome(AttachmentOutcome):
    """An ``image.attach_bytes`` outcome, with the staged image's names."""


@dataclass(frozen=True)
class DetachOutcome(AttachmentOutcome):
    """An ``image.detach`` outcome."""


#: Extension to MIME mapping, used only to *label* the upload. This table
#: accepts nothing and refuses nothing: an unlisted extension maps to
#: ``application/octet-stream`` and the gateway's 4016 answer remains the
#: type policy. D6's accepted-type set will constrain routes, not this map.
_EXTENSION_MIME: Final[dict[str, str]] = {
    ".txt": "text/plain",
    ".md": "text/markdown",
    ".markdown": "text/markdown",
    ".csv": "text/csv",
    ".json": "application/json",
    ".yaml": "text/yaml",
    ".yml": "text/yaml",
    ".toml": "text/toml",
    ".xml": "text/xml",
    ".html": "text/html",
    ".css": "text/css",
    ".py": "text/x-python",
    ".js": "text/javascript",
    ".ts": "text/typescript",
    ".sh": "text/x-sh",
    ".rs": "text/x-rust",
    ".go": "text/x-go",
    ".png": "image/png",
    ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg",
    ".gif": "image/gif",
    ".webp": "image/webp",
    ".bmp": "image/bmp",
    ".svg": "image/svg+xml",
    ".pdf": "application/pdf",
}


def mime_for_filename(filename: str) -> str:
    """Label bytes for upload from the file extension. Never a verdict."""
    return _EXTENSION_MIME.get(Path(filename).suffix.lower(), "application/octet-stream")


def build_data_url(mime: str, raw: bytes) -> str:
    """Encode bytes as the ``data:<mime>;base64,`` URL ``file.attach`` takes."""
    return f"data:{mime};base64," + base64.b64encode(raw).decode("ascii")


def _read_or_outcome(path: Path, method: str) -> bytes | AttachmentOutcome:
    """Read one file, or the outcome that explains why it cannot be read.

    Returns the bytes on success, or an ``error`` outcome when the file is
    missing or unreadable. Nothing is sent in either failure case — the
    gateway never hears about a file we could not read. The ``isinstance``
    check at the call site narrows the union without an ``assert``.
    """
    try:
        is_file = path.is_file()
    except OSError as exc:
        return AttachmentOutcome(
            status="error",
            kind="unreadable-file",
            detail=f"cannot inspect {str(path)!r}: {exc.strerror or exc}",
            method=method,
        )
    if not is_file:
        return AttachmentOutcome(
            status="error",
            kind="missing-file",
            detail=(
                f"no file at {str(path)!r}: nothing was sent; "
                "check the path and retry"
            ),
            method=method,
        )
    try:
        return path.read_bytes()
    except OSError as exc:
        return AttachmentOutcome(
            status="error",
            kind="unreadable-file",
            detail=f"cannot read {str(path)!r}: {exc.strerror or exc}; nothing was sent",
            method=method,
        )


#: Portable-document format is out of scope for v0.6.1 (D6), and
#: ``file.attach`` would accept a PDF — so the refusal lives here, on the
#: file path only, and is explicit about whose decision it is. Images carry
#: no equivalent refusal: the gateway sniffs magic bytes and answers 4016
#: itself. Extension *and* magic bytes, so a renamed PDF does not slip
#: through as a text file.
_EXCLUDED_EXTENSIONS: Final[frozenset[str]] = frozenset({".pdf"})
_PDF_MAGIC: Final[bytes] = b"%PDF-"


def _refuse_excluded_type(
    local: Path, raw: bytes, method: str
) -> AttachmentOutcome | None:
    """Refuse an out-of-scope type Talaria-side, before anything is sent."""
    if local.suffix.lower() in _EXCLUDED_EXTENSIONS or raw.startswith(_PDF_MAGIC):
        return AttachmentOutcome(
            status="error",
            kind="unsupported-type",
            detail=(
                f"Talaria v0.6.1 excludes portable-document format: {local.name} "
                "was not sent; convert it to text or an image and retry"
            ),
            method=method,
        )
    return None


def classify_gateway_error(
    method: str, code: int | None, message: str | None
) -> tuple[AttachmentErrorKind, str]:
    """Map one gateway error onto an honest client-side outcome.

    The gateway's own message is always quoted: it names the extension, the
    cap, or the staging cause, and a client paraphrase would lose exactly
    the actionable part. Retry guidance follows what is known — a malformed
    request will fail identically, an unknown outcome may already have
    staged the file.
    """
    quoted = message or "no detail given"
    if code == 4015:
        return (
            "invalid-request",
            f"the gateway rejected our {method} call as malformed ({quoted}); "
            "retrying the same bytes will fail the same way — this is a "
            "client bug, not a bad file",
        )
    if code == 4016:
        return (
            "unsupported-type",
            f"the gateway does not accept this file ({quoted}); convert it "
            "to a supported type and retry",
        )
    if code == 4017:
        return (
            "bad-payload",
            f"our upload arrived corrupt ({quoted}); retry once — a repeat "
            "means a client encoding bug, not a bad file",
        )
    if code == 4018:
        return (
            "too-large",
            f"the file exceeds the gateway's size cap ({quoted}); shrink it "
            "and retry",
        )
    if code in (5027, 5028):
        return (
            "staging-failed",
            f"the gateway could not stage the file ({quoted}); nothing was "
            "attached — retry once the gateway-side cause (storage, tooling "
            "such as poppler for PDFs) is fixed",
        )
    if code is None:
        return (
            "gateway-error",
            f"the gateway refused the attachment ({quoted})",
        )
    return (
        "gateway-error",
        f"the gateway refused the attachment (code {code}: {quoted})",
    )


def _unknown_outcome(method: str) -> AttachmentOutcome:
    return AttachmentOutcome(
        status="unknown",
        kind="unknown",
        detail=(
            f"the {method} call was interrupted before the gateway answered; "
            "the file may or may not be staged — re-attaching may create a "
            "second staged copy, so verify before retrying"
        ),
        method=method,
    )


def _error_outcome(
    method: str, outcome: RpcOutcome
) -> AttachmentOutcome:
    kind, detail = classify_gateway_error(method, outcome.error_code, outcome.error_message)
    return AttachmentOutcome(
        status="error",
        kind=kind,
        detail=detail,
        method=method,
    )


async def attach_file(
    dispatcher: AttachmentDispatcher,
    path: str | Path,
    *,
    name: str | None = None,
    mime: str | None = None,
    session_id: str | None = None,
    timeout: float | None = ATTACH_TIMEOUT_SECONDS,
) -> FileAttachOutcome:
    """Stage one non-image file via ``file.attach`` (data-URL upload).

    ``path`` names a file on the operator's disk. Missing or unreadable
    files end here with ``missing-file``/``unreadable-file``, and PDFs end
    here with a Talaria-side ``unsupported-type`` refusal — the gateway is
    never called without acceptable bytes to upload. ``session_id`` scopes
    the staging when the caller routes across sessions; ``None`` leaves
    the gateway's default scoping in place. On success ``prompt_text``
    carries the ``@file:`` ref verbatim: the route layer must place it
    into the composer string ``prompt.submit`` sends.
    """
    method = "file.attach"
    local = Path(path)
    read = _read_or_outcome(local, method)
    if isinstance(read, AttachmentOutcome):
        return FileAttachOutcome(
            status=read.status,
            kind=read.kind,
            detail=read.detail,
            method=method,
        )
    refused = _refuse_excluded_type(local, read, method)
    if refused is not None:
        return FileAttachOutcome(
            status=refused.status,
            kind=refused.kind,
            detail=refused.detail,
            method=method,
        )
    params: dict[str, Any] = {
        "data_url": build_data_url(mime or mime_for_filename(local.name), read),
        "name": name or local.name,
    }
    if session_id is not None:
        params["session_id"] = session_id
    outcome = await dispatcher.call(method, params, timeout=timeout)
    if outcome.status == "unknown":
        unknown = _unknown_outcome(method)
        return FileAttachOutcome(
            status=unknown.status,
            kind=unknown.kind,
            detail=unknown.detail,
            method=method,
        )
    if outcome.status != "ok":
        error = _error_outcome(method, outcome)
        return FileAttachOutcome(
            status=error.status,
            kind=error.kind,
            detail=error.detail,
            method=method,
        )
    result = outcome.result or {}
    ref_text = result.get("ref_text")
    gateway_ref = ref_text if isinstance(ref_text, str) else None
    return FileAttachOutcome(
        status="ok",
        kind="attached",
        detail=(
            f"staged {local.name} as {result.get('name', local.name)}; place "
            f"{gateway_ref or 'the @file: ref'} into the composer string "
            "prompt.submit sends — the gateway does not inject it; staged, "
            "not delivered"
        ),
        method=method,
        prompt_text=gateway_ref,
        gateway_ref=gateway_ref,
        name=str(result.get("name", local.name)),
        uploaded=bool(result.get("uploaded", False)),
    )


async def attach_image_file(
    dispatcher: AttachmentDispatcher,
    path: str | Path,
    *,
    filename: str | None = None,
    session_id: str | None = None,
    timeout: float | None = ATTACH_TIMEOUT_SECONDS,
) -> ImageAttachOutcome:
    """Stage one image via ``image.attach_bytes`` (base64 upload).

    Sends ``{session_id, content_base64}`` plus the file's base name when
    the caller passes ``filename`` (the turn contract's optional field,
    I6 addendum on #147): the gateway sniffs the type from magic bytes and
    enforces its own size cap, and its 4016/4018 answers are the type and
    size policy — this module performs no sniffing, no capping, and no
    client-side refusal, so a gateway 4016 always reaches the caller
    honestly mapped. ``prompt_text`` is always ``None``: no ``@file:``
    reference goes in the prompt — ``prompt.submit`` drains the
    staged-image queue itself.
    """
    method = "image.attach_bytes"
    local = Path(path)
    read = _read_or_outcome(local, method)
    if isinstance(read, AttachmentOutcome):
        return ImageAttachOutcome(
            status=read.status,
            kind=read.kind,
            detail=read.detail,
            method=method,
        )
    params: dict[str, Any] = {
        "content_base64": base64.b64encode(read).decode("ascii"),
    }
    if filename is not None:
        params["filename"] = filename
    if session_id is not None:
        params["session_id"] = session_id
    outcome = await dispatcher.call(method, params, timeout=timeout)
    if outcome.status == "unknown":
        unknown = _unknown_outcome(method)
        return ImageAttachOutcome(
            status=unknown.status,
            kind=unknown.kind,
            detail=unknown.detail,
            method=method,
        )
    if outcome.status != "ok":
        error = _error_outcome(method, outcome)
        return ImageAttachOutcome(
            status=error.status,
            kind=error.kind,
            detail=error.detail,
            method=method,
        )
    result = outcome.result or {}
    staged_bytes = result.get("bytes")
    return ImageAttachOutcome(
        status="ok",
        kind="attached",
        detail=(
            f"staged image {local.name}"
            + (f" ({staged_bytes} bytes)" if isinstance(staged_bytes, int) else "")
            + "; put nothing in the prompt — submit drains the staged-image "
            "queue itself; staged, not delivered"
        ),
        method=method,
        gateway_ref=(
            str(result["path"]) if isinstance(result.get("path"), str) else None
        ),
        bytes_staged=staged_bytes if isinstance(staged_bytes, int) else None,
    )


async def detach_image(
    dispatcher: AttachmentDispatcher,
    gateway_path: str,
    *,
    session_id: str | None = None,
    timeout: float | None = ATTACH_TIMEOUT_SECONDS,
) -> DetachOutcome:
    """Remove one staged image via ``image.detach``.

    ``gateway_path`` is the staged path a previous attach confirmed — the
    gateway matches removals against what it staged, not against operator
    disk. Detaching what was never attached still succeeds with
    ``detached`` false in the detail: idempotent removal is an ordinary
    operator gesture, not an error.
    """
    method = "image.detach"
    params: dict[str, Any] = {"path": gateway_path}
    if session_id is not None:
        params["session_id"] = session_id
    outcome = await dispatcher.call(method, params, timeout=timeout)
    if outcome.status == "unknown":
        unknown = _unknown_outcome(method)
        return DetachOutcome(
            status=unknown.status,
            kind=unknown.kind,
            detail=unknown.detail,
            method=method,
        )
    if outcome.status != "ok":
        error = _error_outcome(method, outcome)
        return DetachOutcome(
            status=error.status,
            kind=error.kind,
            detail=error.detail,
            method=method,
        )
    result = outcome.result or {}
    detached = bool(result.get("detached", False))
    return DetachOutcome(
        status="ok",
        kind="detached",
        detail=(
            "image detached"
            if detached
            else "image was not attached; nothing changed"
        ),
        method=method,
    )
