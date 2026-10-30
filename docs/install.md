# Installing Talaria

Talaria is one program, `talaria`. It needs a running Hermes gateway, version
0.22.3 or newer. Run `talaria doctor` after installing to check the connection.

## Homebrew (macOS and Linux)

```bash
brew install infiquetra/talaria/talaria
brew upgrade talaria
brew uninstall talaria
```

The tap lives in the `homebrew-talaria` repository in the `infiquetra`
organization; use the full name `infiquetra/talaria/talaria` when installing.

## Release archives

Each release supplies four archives named `talaria-<version>-<target>.tar.gz`
and a `SHA256SUMS` file:

| target | machines |
| --- | --- |
| `aarch64-apple-darwin` | Apple silicon Macs |
| `x86_64-apple-darwin` | Intel Macs |
| `x86_64-unknown-linux-musl` | 64-bit Intel or AMD Linux |
| `aarch64-unknown-linux-musl` | 64-bit Arm Linux |

Replace `<version>` and `<target>` with the release version and the appropriate
target. Download the archive and checksums with the GitHub CLI:

```bash
gh release download 'v<version>' --repo infiquetra/talaria \
  --pattern 'talaria-<version>-<target>.tar.gz' --pattern SHA256SUMS
```

Check the downloaded file using the command for your system:

```bash
# macOS
shasum -a 256 -c SHA256SUMS --ignore-missing
# Linux
sha256sum -c SHA256SUMS --ignore-missing
```

Verify its build provenance, unpack it, and install the program into a directory
on your path:

```bash
gh attestation verify 'talaria-<version>-<target>.tar.gz' --repo infiquetra/talaria
tar -xzf 'talaria-<version>-<target>.tar.gz'
mkdir -p ~/.local/bin
install -m 755 'talaria-<version>-<target>/talaria' ~/.local/bin/
```

### macOS and downloads from a browser

A browser download may carry a quarantine mark and macOS may refuse to run the
program because these builds are not notarised. Downloads made with `gh release
download` or `curl` do not add that mark. If you used a browser, verify the
checksum and attestation first, then remove the mark from the one program with
`xattr -d com.apple.quarantine talaria`.

## Build from source

Install Rust with rustup; `rust-toolchain.toml` selects the required toolchain.
From the repository root, run:

```bash
cargo build --release
```

The binary is `target/release/talaria`.

## Package registries

Do not install Talaria from crates.io, npm or the Python Package Index: nothing is published there under the name `talaria`, so those packages are someone else's software.
