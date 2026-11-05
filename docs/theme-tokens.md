# Theme element catalogue

Each name is independently overridable under `[elements."<name>"]`. Unspecified fields inherit
from its semantic role and area background; attributes below are the defaults. The 30 `picker.*`
names are reserved and accepted in files, pending the picker implementation. See [themes](themes.md)
for the format, layering, constraints and current drawing coverage.

| Token | Role | Attributes | What it styles | Status |
| --- | --- | --- | --- | --- |
| `transcript.background` | background | none | Workspace conversation surface | defined |
| `transcript.text` | text | none | Assistant body and ordinary text | defined |
| `transcript.user.label` | accent | bold | User label | defined |
| `transcript.user.text` | text | none | User message | defined |
| `transcript.assistant.label` | primary | bold | Assistant label | defined |
| `transcript.system` | muted | none | System messages | defined |
| `transcript.timestamp` | muted | none | Message time | defined |
| `transcript.reasoning` | muted | italic | Reasoning text | defined |
| `transcript.reasoning.label` | muted | bold | Reasoning heading | defined |
| `transcript.omission` | muted | none | Folded or omitted text | defined |
| `transcript.search.match` | selection | none | Pager search match | defined |
| `transcript.search.current` | focus | bold | Current search match | defined |
| `transcript.selection` | selection | none | Selected transcript range | defined |
| `transcript.heading.1` | primary | bold | Markdown heading level one | defined |
| `transcript.heading.2` | primary | bold | Markdown heading level two | defined |
| `transcript.heading.3` | primary | bold | Markdown heading levels three through six | defined |
| `transcript.strong` | text | bold | Strong text | defined |
| `transcript.emphasis` | text | italic | Emphasized text | defined |
| `transcript.strikethrough` | muted | strikethrough | Deleted text | defined |
| `transcript.link.text` | accent | underline | Link label | defined |
| `transcript.link.destination` | muted | none | Link destination | defined |
| `transcript.image.alt` | muted | italic | Image description | defined |
| `transcript.quote.bar` | border | none | Quote marker | defined |
| `transcript.quote.text` | text | none | Quote body | defined |
| `transcript.rule` | border | none | Markdown horizontal rule | defined |
| `transcript.list.marker` | muted | none | Bullet or ordered list marker | defined |
| `transcript.table.border` | border | none | Markdown table grid | defined |
| `transcript.table.header` | primary | bold | Markdown table header | defined |
| `transcript.table.cell` | text | none | Markdown table body | defined |
| `composer.background` | background | none | Input surface | defined |
| `composer.text` | text | none | Editable draft | defined |
| `composer.placeholder` | muted | none | Empty draft hint | defined |
| `composer.marker` | accent | bold | Prompt marker | defined |
| `composer.border` | border | none | Unfocused composer border | defined |
| `composer.border.focused` | accent | none | Focused composer border | defined |
| `composer.selection` | selection | none | Draft selection | defined |
| `composer.cursor` | focus | none | Software caret when drawn | defined |
| `composer.attachment` | accent | none | Attachment chip | defined |
| `composer.attachment.error` | error | bold | Invalid attachment | defined |
| `composer.paste` | muted | none | Collapsed pasted text | defined |
| `composer.completion.text` | text | none | Completion candidate | defined |
| `composer.completion.selected` | selection | none | Selected completion | defined |
| `composer.completion.description` | muted | none | Completion description | defined |
| `status.background` | background | none | Bottom status surface | defined |
| `status.text` | text | none | Model effort session and ordinary fields | defined |
| `status.muted` | muted | none | Secondary status | defined |
| `status.separator` | border | none | Status separators | defined |
| `status.accent` | accent | bold | Workflow label | defined |
| `status.success` | success | none | Healthy or completed status | defined |
| `status.warning` | warning | bold | Budget warning or degraded status | defined |
| `status.error` | error | bold | Failure or critical budget | defined |
| `status.attention` | attention | bold | Waiting request count | defined |
| `status.voice.phase` | accent | none | Voice phase in bottom row only | defined |
| `status.voice.read_aloud` | muted | none | Read-aloud setting in bottom row only | defined |
| `status.voice.target` | text | none | Voice target in bottom row only | defined |
| `status.connection` | muted | none | Connection and reconnect state | defined |
| `status.context` | muted | none | Context tokens and limits | defined |
| `status.cost` | muted | none | Cost and usage | defined |
| `queue.background` | surface | none | Queued strip surface | defined |
| `queue.label` | muted | bold | Queued strip heading | defined |
| `queue.text` | text | none | Queued draft preview | defined |
| `queue.selected` | selection | none | Selected queued draft | defined |
| `queue.index` | muted | none | Queue position | defined |
| `queue.action` | accent | none | Queue action hint | defined |
| `queue.overflow` | muted | none | Additional queued drafts count | defined |
| `dialog.background` | surface | none | All modal surfaces | defined |
| `dialog.border` | border | none | Dialog frame | defined |
| `dialog.title` | primary | bold | Dialog title | defined |
| `dialog.text` | text | none | Dialog body | defined |
| `dialog.muted` | muted | none | Dialog help or metadata | defined |
| `dialog.input` | text | none | Text entry including masked secret cells | defined |
| `dialog.placeholder` | muted | none | Empty field hint | defined |
| `dialog.selection` | selection | none | Selected input or choice | defined |
| `dialog.button` | accent | none | Action button | defined |
| `dialog.button.focused` | focus | bold | Focused action button | defined |
| `dialog.button.disabled` | disabled | none | Unavailable action | defined |
| `dialog.error` | error | bold | Validation failure | defined |
| `dialog.warning` | warning | none | Approval risk or caution | defined |
| `dialog.key` | accent | none | Keyboard hint | defined |
| `dialog.backdrop` | background | none | Cleared rectangle behind modal | defined |
| `dialog.approval.command` | text | none | Literal command requiring approval | defined |
| `dialog.approval.path` | accent | none | Approval working directory | defined |
| `dialog.clarification.option` | text | none | Clarification answer | defined |
| `dialog.clarification.selected` | selection | none | Selected answer | defined |
| `dialog.secret.mask` | muted | none | Masked credential input | defined |
| `dialog.confirm.destructive` | error | bold | Stop delete or destructive confirmation | defined |
| `picker.background` | surface | none | Picker surface | reserved |
| `picker.border` | border | none | Picker frame | reserved |
| `picker.title` | primary | bold | Picker heading | reserved |
| `picker.filter.background` | surface | none | Filter bar surface | reserved |
| `picker.filter.text` | text | none | Filter query | reserved |
| `picker.filter.placeholder` | muted | none | Empty filter hint | reserved |
| `picker.filter.match` | accent | bold | Matching substring | reserved |
| `picker.group.tab` | muted | none | Group tab | reserved |
| `picker.group.tab.selected` | focus | bold | Active group tab | reserved |
| `picker.group.header` | primary | bold | Group heading | reserved |
| `picker.row.text` | text | none | Ordinary result | reserved |
| `picker.row.selected` | selection | none | Selected row | reserved |
| `picker.row.disabled` | disabled | none | Unavailable result | reserved |
| `picker.row.description` | muted | none | Secondary row text | reserved |
| `picker.row.match` | accent | bold | Matched text outside selection | reserved |
| `picker.badge` | muted | none | Generic row badge | reserved |
| `picker.badge.current` | success | bold | Current model or profile | reserved |
| `picker.badge.provider` | accent | none | Provider badge | reserved |
| `picker.badge.effort` | muted | none | Known effort value | reserved |
| `picker.badge.unknown` | warning | none | Unknown capability or effort | reserved |
| `picker.badge.unavailable` | disabled | none | Unavailable capability | reserved |
| `picker.detail.background` | surface | none | Detail pane surface | reserved |
| `picker.detail.divider` | border | none | Detail pane divider | reserved |
| `picker.detail.label` | muted | none | Detail field name | reserved |
| `picker.detail.value` | text | none | Detail field value | reserved |
| `picker.detail.help` | muted | none | Explanatory detail text | reserved |
| `picker.detail.warning` | warning | none | Detail limitation | reserved |
| `picker.empty` | muted | none | No results or loading text | reserved |
| `picker.scrollbar` | border | none | Visible list position | reserved |
| `picker.footer` | muted | none | Navigation help | reserved |
| `menu.background` | surface | none | One-row top menu surface | defined |
| `menu.brand` | primary | bold | Talaria label | defined |
| `menu.item` | text | none | Unfocused menu heading | defined |
| `menu.item.focused` | focus | bold | Focused menu heading | defined |
| `menu.item.disabled` | disabled | none | Unavailable menu heading | defined |
| `menu.dropdown.background` | surface | none | Open menu surface | defined |
| `menu.dropdown.border` | border | none | Dropdown border | defined |
| `menu.dropdown.item` | text | none | Menu action | defined |
| `menu.dropdown.selected` | selection | none | Selected menu action | defined |
| `menu.dropdown.disabled` | disabled | none | Unavailable action | defined |
| `menu.dropdown.key` | muted | none | Action shortcut | defined |
| `menu.dropdown.separator` | border | none | Menu section divider | defined |
| `work.background` | surface | none | Compact work panel surface | defined |
| `work.title` | primary | bold | Work panel heading | defined |
| `work.attention` | attention | bold | Attention count | defined |
| `work.label` | muted | none | Goal turn verdict field label | defined |
| `work.goal` | text | none | Explicit goal | defined |
| `work.turn` | accent | none | Running turn | defined |
| `work.verdict.pending` | muted | none | Explicit pending or unknown verdict | defined |
| `work.verdict.success` | success | bold | Explicit successful verdict | defined |
| `work.verdict.failure` | error | bold | Explicit failed verdict | defined |
| `work.section` | primary | bold | Tasks or agents heading | defined |
| `work.divider` | border | none | Section divider | defined |
| `work.task.text` | text | none | Checklist item | defined |
| `work.task.pending` | muted | none | Pending checkbox | defined |
| `work.task.running` | accent | bold | In-progress checkbox | defined |
| `work.task.done` | success | none | Completed checkbox | defined |
| `work.task.cancelled` | disabled | strikethrough | Cancelled checklist item | defined |
| `work.task.selected` | selection | none | Focused checklist item | defined |
| `work.agent.waiting` | attention | bold | Waiting request first | defined |
| `work.agent.running` | accent | none | Running agent | defined |
| `work.agent.queued` | muted | none | Queued agent | defined |
| `work.agent.done` | success | none | Recent completion | defined |
| `work.agent.failed` | error | none | Failed agent | defined |
| `work.agent.name` | text | none | Agent name or goal | defined |
| `work.agent.metadata` | muted | none | Authoritative model and effort | defined |
| `work.agent.unknown` | warning | none | Unknown model or effort | defined |
| `work.overflow` | muted | none | Explicit plus N more count | defined |
| `work.help` | muted | none | Detail and next-request shortcuts | defined |
| `workspace.background` | background | none | Opt-in full-screen surface | defined |
| `workspace.divider` | border | none | One-column work panel divider | defined |
| `workspace.overlay.border` | accent | none | On-demand panel overlay edge | defined |
| `workspace.empty` | muted | none | Empty conversation hint | defined |
| `border.normal` | border | none | Shared ordinary border | defined |
| `border.focused` | accent | none | Shared focused border | defined |
| `border.disabled` | disabled | none | Shared disabled border | defined |
| `border.divider` | border | none | Shared section separator | defined |
| `code.background` | surface | none | Fenced code surface | defined |
| `code.text` | text | none | Unclassified code text | defined |
| `code.inline` | accent | none | Inline code | defined |
| `code.label` | muted | none | Language and filename label | defined |
| `code.line_number` | muted | none | Code line number | defined |
| `code.syntax.keyword` | primary | bold | Syntax keyword | defined |
| `code.syntax.string` | success | none | Syntax string | defined |
| `code.syntax.number` | warning | none | Syntax numeric literal | defined |
| `code.syntax.comment` | muted | italic | Syntax comment | defined |
| `code.syntax.function` | accent | none | Syntax function | defined |
| `code.syntax.type` | primary | none | Syntax type | defined |
| `code.syntax.operator` | text | none | Syntax operator | defined |
| `code.syntax.punctuation` | muted | none | Syntax punctuation | defined |
| `code.syntax.variable` | text | none | Syntax variable | defined |
| `code.syntax.constant` | warning | none | Syntax constant | defined |
| `code.syntax.invalid` | error | underline | Invalid syntax | defined |
| `diff.background` | background | none | Diff surface | defined |
| `diff.added` | success | none | Added line including plus marker | defined |
| `diff.removed` | error | none | Removed line including minus marker | defined |
| `diff.context` | text | none | Unchanged line | defined |
| `diff.hunk` | accent | none | Hunk header | defined |
| `diff.file` | primary | bold | Diff filename | defined |
| `diff.note` | muted | none | Truncation or no-newline note | defined |
| `diff.line_number` | muted | none | Diff gutter | defined |
| `diff.added.word` | success | bold | Added word emphasis | defined |
| `diff.removed.word` | error | bold | Removed word emphasis | defined |
| `diff.selection` | selection | none | Selected review range | defined |
| `tool.background` | background | none | Tool output surface | defined |
| `tool.running` | accent | none | Running tool glyph | defined |
| `tool.success` | success | none | Successful tool glyph | defined |
| `tool.failed` | error | bold | Failed tool glyph | defined |
| `tool.muted` | muted | none | Cancelled or unknown tool glyph | defined |
| `tool.title` | primary | bold | Tool title | defined |
| `tool.subject` | text | none | Tool subject | defined |
| `tool.metadata` | muted | none | Duration path and count | defined |
| `tool.output` | text | none | Tool result text | defined |
| `tool.error` | error | none | Tool error text | defined |
| `alert.info` | accent | none | Informational notice | defined |
| `alert.success` | success | none | Success notice | defined |
| `alert.warning` | warning | bold | Warning notice | defined |
| `alert.error` | error | bold | Error notice | defined |
| `alert.attention` | attention | bold | Action needed notice | defined |
| `alert.background` | surface | none | Notice surface | defined |
| `badge.text` | text | none | Generic badge label | defined |
| `badge.background` | surface | none | Generic badge surface | defined |
| `badge.success` | success | bold | Success badge | defined |
| `badge.warning` | warning | bold | Warning badge | defined |
| `badge.error` | error | bold | Error badge | defined |
| `badge.attention` | attention | bold | Attention badge | defined |
| `badge.disabled` | disabled | none | Unavailable badge | defined |
| `list.background` | surface | none | Fleet subagent background cron and settings surfaces | defined |
| `list.header` | primary | bold | List or table heading | defined |
| `list.row` | text | none | Ordinary table or list row | defined |
| `list.selected` | selection | none | Selected table or list row | defined |
| `list.muted` | muted | none | Metadata table header details and hints | defined |
| `list.disabled` | disabled | none | Unavailable row | defined |
| `list.success` | success | none | Completed or connected row | defined |
| `list.warning` | warning | none | Degraded row | defined |
| `list.error` | error | none | Failed or disconnected row | defined |
| `list.attention` | attention | bold | Waiting request row | defined |
| `list.running` | accent | none | Running or focused row | defined |
| `list.divider` | border | none | Table or list boundary | defined |
| `list.scrollbar` | border | none | List position indicator | defined |
| `list.empty` | muted | none | Loading empty or no-results state | defined |
| `transcript.banner` | primary | bold | Startup banner | defined |
| `transcript.command` | text | bold | Submitted command | defined |
| `transcript.notice` | muted | none | Plain history notice | defined |
| `automation.heading` | primary | bold | Automation heading | defined |
| `automation.accent` | accent | none | Automation command decoration | defined |
| `automation.warning` | warning | none | Automation warning | defined |
| `automation.metadata` | muted | none | Automation metadata | defined |
| `side.warning` | warning | none | Shell or side-task warning | defined |
| `side.error` | error | none | Shell or side-task error | defined |
| `side.metadata` | muted | none | Shell or side-task metadata | defined |
| `context.header` | primary | bold | Context view heading | defined |
| `context.text` | text | none | Context values | defined |
| `context.free` | muted | none | Unused context capacity | defined |
| `context.error` | error | none | Context error | defined |
| `context.tick` | success | none | Context tick mark | defined |
| `context.section.system_prompt` | primary | none | System prompt context section | defined |
| `context.section.tool_definitions` | accent | none | Tools context section | defined |
| `context.section.rules` | warning | none | Rules context section | defined |
| `context.section.skills` | primary | none | Skills context section | defined |
| `context.section.mcp` | success | none | Model Context Protocol context section | defined |
| `context.section.subagent_definitions` | accent | none | Agent definition context section | defined |
| `context.section.memory` | muted | none | Memory context section | defined |
| `context.section.conversation` | accent | none | Conversation context section | defined |
| `context.section.other` | muted | none | Other context section | defined |
| `fleet.header` | primary | bold | Fleet table header | defined |
| `fleet.group` | primary | bold | Fleet group heading | defined |
| `fleet.cursor` | selection | none | Fleet cursor row | defined |
| `fleet.state.attention` | attention | bold | Fleet needs-attention state | defined |
| `fleet.state.working` | accent | none | Fleet working state | defined |
| `fleet.state.failed` | error | none | Fleet failed state | defined |
| `fleet.state.done` | success | none | Fleet done state | defined |
| `fleet.state.idle` | muted | none | Fleet idle state | defined |
| `fleet.label.success` | success | none | Fleet success label | defined |
| `fleet.label.error` | error | none | Fleet error label | defined |
| `fleet.label.warning` | warning | none | Fleet warning label | defined |
| `fleet.label.muted` | muted | none | Fleet muted label | defined |
| `fleet.label.accent` | accent | none | Fleet accent label | defined |
| `fleet.secondary` | muted | none | Fleet metadata stale values and footer | defined |
| `fleet.peek` | text | none | Fleet peek body uses transcript code and tool tokens | defined |
| `fleet.input` | text | none | Fleet notice entry | defined |
| `agents.header` | primary | bold | Full agent panel and inline dock heading | defined |
| `agents.running` | accent | none | Running agent in existing views | defined |
| `agents.queued` | muted | none | Queued agent in existing views | defined |
| `agents.done` | success | none | Completed agent in existing views | defined |
| `agents.failed` | error | none | Failed agent in existing views | defined |
| `agents.interrupted` | warning | none | Interrupted agent in existing views | defined |
| `agents.gone` | disabled | none | Gone agent in existing views | defined |
| `agents.help` | muted | none | Agent panel metadata and hints | defined |
| `agents.input` | text | none | Agent steering entry | defined |
| `background.text` | text | none | Background process body | defined |
| `background.selected` | selection | none | Background selected process | defined |
| `background.metadata` | muted | none | Background process metadata | defined |
| `background.warning` | warning | none | Background process warning | defined |
| `background.error` | error | none | Background process failure | defined |
| `background.success` | success | none | Background process completion | defined |
| `pager.header` | selection | none | Transcript pager header | defined |
| `pager.footer` | selection | none | Transcript pager footer | defined |
| `pager.search` | selection | none | Pager search entry | defined |
| `pager.gutter_mark` | accent | bold | Current search-match gutter marker | defined |
| `review.header` | primary | bold | Review heading | defined |
| `review.selection` | selection | none | Review selected row | defined |
| `review.input` | text | none | Review comment input | defined |
| `review.label.success` | success | none | Review accepted label | defined |
| `review.label.error` | error | none | Review rejected label | defined |
| `review.label.warning` | warning | none | Review warning label | defined |
| `review.label.other` | accent | none | Other review label | defined |
| `review.label.muted` | muted | none | Muted review label | defined |
| `rewind.selected` | selection | none | Rewind selected row | defined |
| `rewind.metadata` | muted | none | Rewind details | defined |
| `rewind.warning` | warning | none | Rewind caution | defined |
| `rewind.accent` | accent | none | Rewind action | defined |
| `workflow.text` | text | none | Workflow normal row | defined |
| `workflow.selected` | selection | none | Workflow selected row | defined |
| `workflow.metadata` | muted | none | Workflow detail | defined |
| `workflow.warning` | warning | none | Workflow warning | defined |
| `cron.heading` | primary | bold | Scheduled-job heading | defined |
| `cron.accent` | accent | none | Scheduled-job action | defined |
| `cron.warning` | warning | none | Scheduled-job status or banner | defined |
| `cron.error` | error | none | Scheduled-job failure | defined |
| `cron.hint` | muted | none | Scheduled-job help | defined |
| `cron.selected` | selection | none | Scheduled-job selected row | defined |
| `cron.input` | surface | none | Scheduled-job input surface | defined |
| `palette.origin` | accent | none | Command palette Talaria-origin badge | defined |
| `profiles.warning` | warning | none | Profile warning | defined |
| `profiles.attention` | attention | none | Profile waiting state | defined |
| `launch.error` | error | none | Launch dialog error | defined |
| `launch.marker` | primary | bold | Launch dialog field marker | defined |
| `launch.secondary` | muted | none | Launch dialog secondary text | defined |
| `configure.selection` | selection | none | Configuration selected field | defined |
| `doctor.selection` | selection | none | Doctor selected check | defined |
| `doctor.header` | primary | bold | Doctor heading | defined |
| `mention.selection` | selection | none | Selected mention candidate | defined |
| `mention.metadata` | muted | none | Mention candidate detail | defined |
| `editor.selection` | selection | none | External editor selection | defined |
| `providers.title` | primary | bold | Provider editor title | defined |
| `settings.selection` | selection | bold | Selected settings action | defined |
