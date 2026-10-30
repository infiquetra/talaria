# Packaging Tool

`tools/package/package.py` builds, checks, and packs release archives for the
four supported platforms (`aarch64-apple-darwin`, `x86_64-apple-darwin`,
`x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`), generates
`SHA256SUMS`, plans release asset uploads without replacing existing files, and
renders the Homebrew formula for `infiquetra/homebrew-talaria`.

## Subcommands

```bash
python3 -I tools/package/package.py targets [--json]
python3 -I tools/package/package.py build --target <target>
python3 -I tools/package/package.py check-binary --target <target> --version <version> <path>
python3 -I tools/package/package.py archive --target <target> --version <version> --binary <path> --out <dir>
python3 -I tools/package/package.py checksums <dir>
python3 -I tools/package/package.py version-gate --tag <tag> --workspace-version <workspace-version>
python3 -I tools/package/package.py formula --version <version> --sums <sums-file> [--base-url <url>]
python3 -I tools/package/package.py plan-upload <sums-file> <existing-assets-dir>
```

## Tests

```bash
PACKAGE_REQUIRE_RUBY=1 python3 -I -m unittest discover -s tools/package/tests -t tools/package
PACKAGE_LIVE=1 python3 -I -m unittest discover -s tools/package/tests -t tools/package -p test_live_build.py
```
