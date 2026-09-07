# Recorded Open VSX responses (2026-09-07, `talaria-theme-import` UA).

Both files are byte captures from the live service for
dracula-theme/theme-dracula/2.25.1, recorded to pin the registry's routing
in `tests/ui/test_theme_import.py`:

- `dracula-theme-2.25.1-package.json`: `GET
  /api/dracula-theme/theme-dracula/2.25.1/file/package.json` answers 302 to
  `openvsx.eclipsecontent.org`, followed to HTTP 200. This is why the old
  `_file_url` shape kept working for the manifest read.
- `dracula-theme-2.25.1-dracula.json`: `GET
  /vscode/unpkg/dracula-theme/theme-dracula/2.25.1/extension/theme/dracula.json`
  answers HTTP 200. The same file at
  `/api/dracula-theme/theme-dracula/2.25.1/file/theme/dracula.json` answers
  HTTP 404 — the defect the unpkg shape repairs.
