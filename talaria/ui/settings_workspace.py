"""Bounded settings workspace: target header, searchable rows, overlays.

Renders a domain :class:`~talaria.domain.settings.SettingsWorkspaceView` and
reports typed commands through ``on_command``. Widgets do not hold protocol
or session state (ADR-0002). Overlay cancellation never commits.
"""

from __future__ import annotations

from collections.abc import Callable, Mapping, Sequence
from dataclasses import replace
from typing import Any

from textual import events, on
from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.css.query import NoMatches
from textual.screen import ModalScreen
from textual.widget import Widget
from textual.widgets import Button, Input, Static

from talaria.domain.settings import (
    ConfigTarget,
    FieldSaveResult,
    GatewayLifecyclePrompt,
    Reauthenticate,
    ResetConfirmView,
    RestartConfirmView,
    RestartPlan,
    RevealSecretView,
    SettingsDocument,
    SettingsState,
    SettingsWorkspaceView,
    TargetSwitchPrompt,
    apply_settings_response,
    begin_settings_request,
    build_config_patch,
    fail_settings_save,
    project_save_summary,
    record_settings_load,
    request_settings_switch,
    select_settings_target,
    stage_settings_edit,
)
from talaria.domain.settings_commands import (
    LoadTarget,
    ResetConfig,
    RestartGateway,
    RevealEnv,
    SaveConfig,
    SetModel,
    StartGateway,
    StopGateway,
    WakeWord,
)
from talaria.ui.config_view import ConfigViewResult
from talaria.ui.literal import literal_text
from talaria.ui.settings_overlays import (
    GatewayLifecycleOverlay,
    GatewayLifecycleResult,
    ModelPickerOverlay,
    ModelPickResult,
    ResetConfirmOverlay,
    ResetConfirmResult,
    RestartConfirmOverlay,
    RestartConfirmResult,
    RevealSecretOverlay,
    RevealSecretResult,
    SwitchChoice,
    TargetSwitchOverlay,
)
from talaria.ui.settings_widgets import (
    SettingsGroupWidget,
    TalariaOwnedSettings,
    coerce_row_value,
    display_row_value,
    group_widget_id,
    nested_assign,
)

__all__ = ("SettingsWorkspaceScreen",)


class SettingsWorkspaceScreen(ModalScreen[ConfigViewResult | None]):
    """Three-region settings workspace with focused overlays.

    Widget ids pinned by Test Author One: ``#settings-search``,
    ``#settings-save``, ``#settings-discard``, ``#settings-group-<owner>``,
    ``#wake-toggle``. The Talaria-owned branch (``#interval``,
    ``#apply-user``, ``#theme-picker``) mounts only when the app supplies
    process state.
    """

    BINDINGS = [
        ("escape", "cancel_view", "Cancel"),
        ("slash", "focus_search", "Search"),
    ]

    DEFAULT_CSS = """
    SettingsWorkspaceScreen {
        align: left top;
    }
    SettingsWorkspaceScreen > #settings-root {
        width: 100%;
        height: 100%;
        background: $surface;
        padding: 0 1;
    }
    SettingsWorkspaceScreen #settings-header {
        dock: top;
        height: auto;
        color: $text;
    }
    SettingsWorkspaceScreen #settings-target-header {
        height: auto;
    }
    SettingsWorkspaceScreen #settings-footer {
        dock: bottom;
        height: auto;
    }
    SettingsWorkspaceScreen #settings-body {
        height: 1fr;
    }
    SettingsWorkspaceScreen #settings-dynamic-groups {
        height: auto;
        width: 100%;
    }
    SettingsWorkspaceScreen .settings--title {
        color: $accent;
        text-style: bold;
    }
    SettingsWorkspaceScreen .settings--group-title {
        color: $accent;
        text-style: bold;
        margin-top: 1;
    }
    SettingsWorkspaceScreen .settings--help {
        color: $text-muted;
    }
    SettingsWorkspaceScreen .settings--notice {
        color: $warning;
    }
    SettingsWorkspaceScreen .settings--validation {
        color: $error;
    }
    SettingsWorkspaceScreen .settings--readonly {
        color: $text-muted;
    }
    SettingsWorkspaceScreen .settings--nav-wide {
        display: none;
    }
    """

    def __init__(
        self,
        view: SettingsWorkspaceView,
        on_command: Callable[[object], None] | None = None,
        *,
        theme_name: str | None = None,
        status_command: str = "",
        status_interval_seconds: int = 5,
        status_segments: Sequence[str] = (),
        scopes: Mapping[tuple[str, str], str] | None = None,
        segments_now: Sequence[str] = (),
        connection_id: str = "",
        **kwargs: object,
    ) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self._view = view
        self._on_command = on_command
        # Live /config always passes the connection id. UI tests omit it; the
        # fixture label equals the id, so this default keeps those screens
        # constructible without reading the label at write time.
        self._connection_id = connection_id.strip() or view.header.connection_label
        self._theme_name = theme_name
        self._status_command = status_command
        self._status_interval_seconds = status_interval_seconds
        self._status_segments = tuple(status_segments)
        self._scopes = scopes
        self._segments_now = tuple(segments_now)
        self._header_line: Static | None = None
        self._notice_line: Static | None = None
        self._summary_line: Static | None = None
        self._search: Input | None = None
        self._groups: list[SettingsGroupWidget] = []
        self._talaria: TalariaOwnedSettings | None = None
        self._return_focus: Widget | None = None
        self._nav_collapsed = False
        self._reveal_key = ""
        self._restart_plan: RestartPlan | None = None
        self._switch_target: ConfigTarget | None = None
        self._lifecycle_action = ""
        self._target_picker: Vertical | None = None
        self._picker_open = False
        self._pending_loaded_view: SettingsWorkspaceView | None = None
        self._reconcile_seq = 0
        self._load_state = view.load_state
        self._settings_state = SettingsState(
            selected=ConfigTarget(
                connection_id=self._connection_id,
                profile_name=view.header.selected_profile,
            )
        )

    def _selected_target(self) -> ConfigTarget:
        return ConfigTarget(
            connection_id=self._connection_id,
            profile_name=self._view.header.selected_profile,
        )

    def _header_text(self) -> str:
        header = self._view.header
        return (
            f"{header.connection_label}  "
            f"current: {header.current_profile}\n"
            f"selected: {header.selected_profile}  "
            f"{header.auth_mode}  {header.hermes_version}"
        )

    def _summary_text(self) -> str:
        summary = self._view.summary
        if summary is None:
            return ""
        parts: list[str] = []
        for group in summary.groups:
            parts.extend(detail for detail in group.details if detail)
        return "  ".join(parts)

    def compose(self) -> ComposeResult:
        with Vertical(id="settings-root"):
            with Vertical(id="settings-header"):
                yield Static(
                    literal_text("settings"),
                    markup=False,
                    classes="settings--title",
                )
                self._header_line = Static(
                    literal_text(self._header_text()),
                    markup=False,
                    id="settings-target-header",
                )
                yield self._header_line
                yield Button("Target", id="settings-target", compact=True)
                self._target_picker = Vertical(id="settings-target-picker")
                yield self._target_picker
                yield Static(
                    literal_text("owner · category"),
                    markup=False,
                    classes="settings--nav-wide settings--help",
                    id="settings-nav",
                )
                self._search = Input(
                    value=self._view.search_text,
                    id="settings-search",
                    placeholder="search labels, help, keys",
                )
                yield self._search
                self._notice_line = Static(
                    literal_text(self._view.notice),
                    markup=False,
                    classes="settings--notice",
                    id="settings-notice",
                )
                yield self._notice_line
                self._summary_line = Static(
                    literal_text(self._summary_text()),
                    markup=False,
                    classes="settings--notice",
                    id="settings-summary",
                )
                yield self._summary_line
            with VerticalScroll(id="settings-body"):
                if self._theme_name is not None and self._scopes is not None:
                    self._talaria = TalariaOwnedSettings(
                        theme_name=self._theme_name,
                        status_command=self._status_command,
                        status_interval_seconds=self._status_interval_seconds,
                        status_segments=self._status_segments,
                        scopes=self._scopes,
                        segments_now=self._segments_now,
                        on_dismiss=self._dismiss_from_branch,
                        id="settings-talaria-branch",
                    )
                    yield self._talaria
                with Vertical(id="settings-dynamic-groups"):
                    used_ids: set[str] = set()
                    for group in self._view.groups:
                        widget = SettingsGroupWidget(
                            group.owner,
                            group.title,
                            group.rows,
                            secrets=self._view.secrets,
                            on_reveal=self.open_reveal,
                            widget_id=group_widget_id(
                                group.owner, group.title, used=used_ids
                            ),
                        )
                        self._groups.append(widget)
                        yield widget
            with Horizontal(id="settings-footer"):
                yield Button("Save", id="settings-save", compact=True)
                yield Button("Discard", id="settings-discard", compact=True)
                if self._view.wake_state is not None:
                    yield Button("Wake word", id="wake-toggle", compact=True)
                if self._view.gateway_running is not None:
                    yield Button("Start", id="settings-start", compact=True)
                    yield Button("Stop", id="settings-stop", compact=True)
                if self._view.auth_state == "reauth":
                    yield Button(
                        "Re-authenticate", id="settings-reauth", compact=True
                    )

    def on_mount(self) -> None:
        self._apply_search(self._view.search_text)
        self._apply_layout(self.size.width)
        self._apply_lifecycle_controls()
        if self._target_picker is not None:
            self._target_picker.display = False

    @on(Button.Pressed, "#settings-target")
    def _toggle_target_picker(self) -> None:
        picker = self._target_picker
        if picker is None:
            return
        if self._picker_open:
            picker.display = False
            self._picker_open = False
            return
        picker.remove_children()
        for option in self._view.target_options:
            label = f"{option.connection_id} / {option.profile_name}"
            picker.mount(Button(label, compact=True, classes="settings--target-option"))
        picker.display = True
        self._picker_open = True

    @on(Button.Pressed)
    def _choose_target_option(self, event: Button.Pressed) -> None:
        if "settings--target-option" not in event.button.classes:
            return
        event.stop()
        label = str(event.button.label).strip()
        connection_id, _, profile_name = label.partition(" / ")
        if self._target_picker is not None:
            self._target_picker.display = False
            self._picker_open = False
        if not connection_id or not profile_name:
            return
        self.open_target_switch(
            ConfigTarget(connection_id=connection_id, profile_name=profile_name)
        )

    def on_resize(self, event: events.Resize) -> None:
        self._apply_layout(event.size.width)

    def _apply_layout(self, width: int) -> None:
        collapsed = width < 100
        self._nav_collapsed = collapsed
        try:
            nav = self.query_one("#settings-nav", Static)
        except NoMatches:
            return
        nav.display = not collapsed

    def on_key(self, event: events.Key) -> None:
        if event.key in ("/", "slash") and not isinstance(self.focused, Input):
            event.stop()
            event.prevent_default()
            self.action_focus_search()
            return
        if event.key in ("escape", "tab", "shift+tab", "slash"):
            return
        event.stop()

    def action_focus_search(self) -> None:
        if isinstance(self.focused, Input):
            return
        if self._search is not None:
            self._search.focus()

    def action_cancel_view(self) -> None:
        notice = self._talaria.notice_text if self._talaria is not None else ""
        if not notice:
            notice = self._view.notice
        if notice:
            self.dismiss(ConfigViewResult(notice=notice))
        else:
            self.dismiss(None)

    def _dismiss_from_branch(self, result: ConfigViewResult | None) -> None:
        self.dismiss(result)

    def begin_settings_reread(self, target: ConfigTarget) -> int:
        """Open a generation so the post-save GET can land (R9 / C10)."""
        self._settings_state, generation = begin_settings_request(
            self._settings_state, target
        )
        return generation

    def apply_settings_reread(
        self,
        target: ConfigTarget,
        generation: int,
        saved: Mapping[str, Any],
        effective: Mapping[str, Any],
        defaults: Mapping[str, Any] | None = None,
    ) -> None:
        self._settings_state = apply_settings_response(
            self._settings_state,
            target=target,
            generation=generation,
            saved=saved,
            effective=effective,
            defaults=defaults,
        )

    def paint_save_summary(
        self, results: Sequence[FieldSaveResult], *, notice: str
    ) -> None:
        self.update_view(
            replace(
                self._view,
                summary=project_save_summary(results=results),
                notice=notice,
            )
        )

    def update_view(self, view: SettingsWorkspaceView) -> None:
        """Refresh header, notice, and summary. Editors keep their values."""
        self._view = view
        self._load_state = view.load_state
        if self._header_line is not None:
            self._header_line.update(literal_text(self._header_text()))
        if self._notice_line is not None:
            self._notice_line.update(literal_text(view.notice))
        if self._summary_line is not None:
            self._summary_line.update(literal_text(self._summary_text()))
        self._apply_lifecycle_controls()

    def _apply_lifecycle_controls(self) -> None:
        running = self._view.gateway_running
        try:
            start = self.query_one("#settings-start", Button)
            stop = self.query_one("#settings-stop", Button)
        except NoMatches:
            start = None
            stop = None
        if start is not None:
            start.display = running is False
        if stop is not None:
            stop.display = running is True
        try:
            reauth = self.query_one("#settings-reauth", Button)
        except NoMatches:
            return
        reauth.display = self._view.auth_state == "reauth"

    def mark_reauth(self, detail: str) -> None:
        """Show re-authenticate state. Pending editor values stay put."""
        notice = f"re-authenticate — edits retained ({detail})"
        self.update_view(replace(self._view, auth_state="reauth", notice=notice))
        if list(self.query("#settings-reauth")):
            return
        try:
            footer = self.query_one("#settings-footer")
        except NoMatches:
            return
        footer.mount(Button("Re-authenticate", id="settings-reauth", compact=True))

    def apply_gateway_running(self, running: bool, *, notice: str) -> None:
        self.update_view(
            replace(self._view, gateway_running=running, notice=notice)
        )

    def _row_widgets(self) -> list[Any]:
        widgets: list[Any] = []
        for group in self._groups:
            widgets.extend(group.row_widgets)
        return widgets

    def _apply_search(self, query: str) -> None:
        needle = query.strip().lower()
        for widget in self._row_widgets():
            row = widget.row
            match = (
                not needle
                or needle in row.label.lower()
                or needle in row.help_text.lower()
                or needle in row.key.lower()
            )
            widget.display = match

    def on_input_changed(self, event: Input.Changed) -> None:
        if event.input.id == "settings-search":
            self._apply_search(event.value)

    def _emit(self, command: object) -> None:
        if self._on_command is not None:
            self._on_command(command)

    def _validation_blocks_save(self) -> bool:
        for group in self._view.groups:
            for row in group.rows:
                if row.validation_message:
                    return True
        return False

    def _edits_and_saved(self) -> tuple[dict[str, object], dict[str, object]]:
        edits: dict[str, object] = {}
        saved: dict[str, object] = {}
        for widget in self._row_widgets():
            nested_assign(saved, widget.row.key, widget.row.saved_value)
            if widget.editor is None:
                continue
            edits[widget.row.key] = coerce_row_value(widget.row, widget.editor.value)
        return edits, saved

    def _emit_save(self) -> None:
        if self._validation_blocks_save():
            return
        edits, saved = self._edits_and_saved()
        patch = build_config_patch(saved=saved, edits=edits)
        self._emit(SaveConfig(target=self._selected_target(), patch=patch))

    def _restore_editors(self) -> None:
        for widget in self._row_widgets():
            if widget.editor is None:
                continue
            widget.editor.value = display_row_value(widget.row)

    @on(Button.Pressed, "#settings-save")
    def _save(self) -> None:
        self._emit_save()

    @on(Button.Pressed, "#settings-discard")
    def _discard(self) -> None:
        self._restore_editors()

    @on(Button.Pressed, "#wake-toggle")
    def _wake(self) -> None:
        action = "stop" if self._view.wake_state == "on" else "start"
        self._emit(WakeWord(target=self._selected_target(), action=action))

    @on(Button.Pressed, "#settings-start")
    def _start_gateway(self) -> None:
        self.open_gateway_lifecycle("start")

    @on(Button.Pressed, "#settings-stop")
    def _stop_gateway(self) -> None:
        self.open_gateway_lifecycle("stop")

    @on(Button.Pressed, "#settings-reauth")
    def _reauth(self) -> None:
        self._emit(Reauthenticate(target=self._selected_target()))

    def open_gateway_lifecycle(self, action: str) -> None:
        running = self._view.gateway_running is True
        prompt = GatewayLifecyclePrompt(
            target=self._selected_target(),
            action=action,
            running=running,
        )
        self._push_overlay(
            GatewayLifecycleOverlay(prompt, on_result=self._on_lifecycle_result)
        )
        self._lifecycle_action = action

    def _on_lifecycle_result(self, result: GatewayLifecycleResult | None) -> None:
        self._restore_focus()
        action = self._lifecycle_action
        if result is None or not result.confirmed:
            return
        target = self._selected_target()
        if action == "start":
            self._emit(StartGateway(target=target))
        elif action == "stop":
            self._emit(StopGateway(target=target))

    def _remember_focus(self) -> None:
        focused = self.focused
        self._return_focus = focused if isinstance(focused, Widget) else None

    def _restore_focus(self) -> None:
        widget = self._return_focus
        if widget is not None and widget.is_mounted:
            widget.focus()
        elif widget is not None:
            self._return_focus = None

    def _push_overlay(self, overlay: ModalScreen[Any]) -> None:
        self._remember_focus()
        self.app.push_screen(overlay)

    def open_target_switch(self, new_target: ConfigTarget) -> None:
        self._switch_target = new_target
        prompt = TargetSwitchPrompt(
            old_target=self._selected_target(),
            new_target=new_target,
            pending_count=max(self._view.pending_count, 1),
        )
        self._push_overlay(
            TargetSwitchOverlay(prompt, on_result=self._on_switch_result)
        )

    def _state_with_editor_pending(self) -> SettingsState:
        """Stage live editor values so the switch reducer sees the same edits."""
        state = self._settings_state
        target = self._selected_target()
        if state.selected is None:
            state = select_settings_target(state, target)
        edits, saved = self._edits_and_saved()
        document = SettingsDocument(target=target, saved=saved, effective=saved)
        state = replace(
            state, documents={**dict(state.documents), target: document}
        )
        for key, value in edits.items():
            state = stage_settings_edit(state, target=target, key=key, value=value)
        self._settings_state = state
        return state

    def _apply_selected(self, target: ConfigTarget) -> None:
        self._connection_id = target.connection_id
        header = replace(
            self._view.header,
            selected_profile=target.profile_name,
            shows_both_names=self._view.header.current_profile != target.profile_name,
        )
        self.update_view(replace(self._view, header=header))

    def fail_save(self, target: ConfigTarget) -> None:
        self._settings_state = fail_settings_save(
            self._settings_state, target=target
        )

    def apply_loaded_view(self, view: SettingsWorkspaceView, connection_id: str) -> None:
        """Reconcile the dynamic Hermes subtree. The Talaria branch stays."""
        self._bind_loaded_view(view, connection_id)
        self._reconcile_seq += 1
        if self.is_attached:
            self.run_worker(
                self._reconcile_dynamic_groups(self._reconcile_seq),
                exclusive=True,
                group="settings-reconcile",
            )

    async def reconcile_loaded_view(
        self, view: SettingsWorkspaceView, connection_id: str
    ) -> None:
        """Await group remount before a target load reports complete."""
        self._bind_loaded_view(view, connection_id)
        self._reconcile_seq += 1
        await self._reconcile_dynamic_groups(self._reconcile_seq)

    def _bind_loaded_view(
        self, view: SettingsWorkspaceView, connection_id: str
    ) -> None:
        self._connection_id = connection_id
        saved: dict[str, Any] = {}
        for group in view.groups:
            for row in group.rows:
                nested_assign(saved, row.key, row.saved_value)
        self._settings_state = record_settings_load(
            self._settings_state,
            target=ConfigTarget(
                connection_id=connection_id,
                profile_name=view.header.selected_profile,
            ),
            saved=saved,
            effective=saved,
        )
        self._pending_loaded_view = view

    async def _reconcile_dynamic_groups(self, seq: int) -> None:
        if seq != self._reconcile_seq:
            return
        view = self._pending_loaded_view or self._view
        self._wipe_active_reveal()
        try:
            container = self.query_one("#settings-dynamic-groups")
        except NoMatches:
            self.update_view(view)
            return
        await container.remove_children()
        if seq != self._reconcile_seq:
            return
        self._groups = []
        used_ids: set[str] = set()
        for group in view.groups:
            widget = SettingsGroupWidget(
                group.owner,
                group.title,
                group.rows,
                secrets=view.secrets,
                on_reveal=self.open_reveal,
                widget_id=group_widget_id(group.owner, group.title, used=used_ids),
            )
            self._groups.append(widget)
            await container.mount(widget)
        if seq != self._reconcile_seq:
            return
        self.update_view(view)
        search = self._search.value if self._search is not None else view.search_text
        self._apply_search(search)
        self._repair_focus_after_remount()

    def _wipe_active_reveal(self) -> None:
        screen = self.app.screen
        if isinstance(screen, RevealSecretOverlay):
            screen.abandon()

    def _repair_focus_after_remount(self) -> None:
        if self._return_focus is not None and not self._return_focus.is_mounted:
            self._return_focus = None
        focused = self.focused
        if focused is not None and focused.is_mounted:
            return
        try:
            self.query_one("#settings-target", Button).focus()
        except NoMatches:
            for widget in self._row_widgets():
                if widget.editor is not None and widget.display:
                    widget.editor.focus()
                    return

    def _on_switch_result(self, result: SwitchChoice | None) -> None:
        self._restore_focus()
        new_target = self._switch_target
        if new_target is None:
            return
        choice = "stay" if result is None else result.choice
        if choice not in {"save", "discard", "stay"}:
            choice = "stay"
        state = self._state_with_editor_pending()
        next_state, issued = request_settings_switch(
            state, new_target=new_target, choice=choice
        )
        self._settings_state = next_state
        for command in issued:
            self._emit(command)
        if choice == "stay":
            return
        if choice == "save":
            return
        if choice == "discard":
            self._restore_editors()
            if next_state.selected is not None:
                self._apply_selected(next_state.selected)
                self._emit(LoadTarget(target=next_state.selected))

    def open_reset(self) -> None:
        confirm = ResetConfirmView(
            target=self._selected_target(),
            patch=self._view.reset_patch,
        )
        self._push_overlay(
            ResetConfirmOverlay(confirm, on_result=self._on_reset_result)
        )

    def _on_reset_result(self, result: ResetConfirmResult | None) -> None:
        self._restore_focus()
        if result is None or not result.confirmed:
            return
        self._emit(
            ResetConfig(
                target=self._selected_target(),
                patch=dict(self._view.reset_patch),
            )
        )

    def open_restart(self, plan: RestartPlan) -> None:
        self._restart_plan = plan
        confirm = RestartConfirmView(plan=plan)
        self._push_overlay(
            RestartConfirmOverlay(confirm, on_result=self._on_restart_result)
        )

    def _on_restart_result(self, result: RestartConfirmResult | None) -> None:
        self._restore_focus()
        if result is None or not result.confirmed or self._restart_plan is None:
            return
        self._emit(
            RestartGateway(
                target=self._selected_target(),
                plan=self._restart_plan,
            )
        )

    def open_reveal(self, key: str) -> None:
        secret = self._view.secrets.get(key, (False, ""))
        confirm = RevealSecretView(key=key, masked=secret[1])
        self._push_overlay(
            RevealSecretOverlay(confirm, on_result=self._on_reveal_result)
        )
        self._reveal_key = key

    def _on_reveal_result(self, result: RevealSecretResult | None) -> None:
        self._restore_focus()
        if result is None or not result.reveal:
            return
        self._emit(
            RevealEnv(target=self._selected_target(), key=self._reveal_key)
        )

    def open_model_picker(self) -> None:
        picker = self._view.model_picker
        if picker is None:
            return
        self._push_overlay(
            ModelPickerOverlay(picker, on_result=self._on_model_result)
        )

    def _on_model_result(self, result: ModelPickResult | None) -> None:
        self._restore_focus()
        if result is None:
            return
        self._emit(
            SetModel(
                target=self._selected_target(),
                provider=result.provider,
                model=result.model,
            )
        )
