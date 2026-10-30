# Remote gateways

Talaria can connect to an operator-managed Hermes gateway through a TLS tunnel. On the
remote machine, run the pinned Hermes release, enable an interactive dashboard sign-in
provider, and set `dashboard.public_url` to the tunnel's public HTTPS address. Forward the
tunnel to the gateway's **own** listener, which carries the session authority. Its port is
random at each gateway start; arrange the tunnel target accordingly. A standalone dashboard
listener cannot serve a Talaria session. Talaria does not launch or configure the tunnel.

```sh
talaria remote add work https://gateway.example.com/hermes --provider basic
talaria remote login work
talaria --remote work --profile fixture
```

The optional path prefix is preserved. Remote names contain lowercase letters, digits and
hyphens, start with a letter or digit, and have at most 32 characters. `remote add` checks for
a gated listener offering `native_pkce`; `--no-check` defers that probe. HTTPS is required.
An SSH local forward may use `http://127.0.0.1:<port>` or `http://[::1]:<port>`; `localhost`
and other plain HTTP hosts are refused. Certificates are verified against platform roots.
There is no switch to disable verification.

Sign-in opens the system browser (`BROWSER`, otherwise `open` on macOS or `xdg-open` on Linux).
With `--no-browser`, or if opening fails, the authorization address is printed once to stderr.
Open that address to sign in. On a terminal without a local browser, use:

```sh
talaria remote login work --paste
```

Open the displayed address in a browser, then paste the address the browser was redirected
to into the waiting terminal. Echo is disabled during paste. The callback address includes
a code that expires after 120 seconds; do not share it. The loopback callback and paste race,
and the first one wins. Ctrl+C cancels; sign-in otherwise times out after ten minutes.
Hermes has no device-code sign-in at the pinned release.

```sh
talaria remote list
talaria remote status work
talaria remote logout work
talaria remote remove work
```

`list` reads saved state without making network requests. `status` refreshes when needed and
shows the verified user, display name, provider and expiry. Logout deletes local credentials;
Hermes's native routes provide no revoke endpoint. Removing a remote also deletes its tokens.

The configuration root is a nonempty `TALARIA_CONFIG_DIR`, otherwise `~/.talaria`.
`remotes.toml` contains the names, URLs and optional providers. Credentials are stored separately
in `remote-tokens/<name>.json`: directory mode 0700, file mode 0600, atomically replaced. A token
file readable by other users is refused; its mode is never silently repaired. An unknown token
file version appears signed out.

Attach never opens a browser. Missing credentials or an expired refresh session fail with exit
1 and a `talaria remote login <name>` next step. A temporarily unreachable provider keeps saved
credentials. Each reconnect refreshes if needed, mints a new single-use ticket, and checks the
selected profile. Long-lived tokens are sent only in HTTPS bodies or the Authorization header;
only the 30-second ticket goes in the WebSocket query.

If the WebSocket upgrade is refused, check the tunnel target, `dashboard.public_url` host
acceptance, and whether chat is enabled. HTTP 404 means no `/api/ws` at that base; HTTP 502–504
means the tunnel or gateway is unavailable. A missing session authority means the tunnel must
point at the gateway's own listener. A profile mismatch means the remote does not serve the
selected profile. The local fleet and remote workflows are outside this connection mode.

Recordings use the usual `--record` option. Credentials are redacted before disk. HTTP traffic
is captured only by the explicit `remote_probe` example, through the remote capture sanitizer.
The probe requires operator-managed remote infrastructure and a fresh browser sign-in:

```sh
cargo run -p hermes-client --example remote_probe -- --remote work
TALARIA_LIVE_REMOTE=work cargo test -p hermes-client --test live_remote -- --ignored
```

The probe writes sanitized `fixtures/remote/probe.json` and creates one empty session in the
`fixture` profile to verify routing. Neither mock tests nor a skipped live test establish live
tunnel acceptance.
