# Self-hosting fond securely

`fond serve` launches a small web UI so household members can browse and cook
from a browser instead of the CLI. By default it binds to `127.0.0.1` — only
reachable from the same machine — and needs no configuration.

The moment you expose it to anyone else (a phone on the couch, a partner's
laptop, a home server reachable across your LAN), it is serving your **entire
recipe collection and authored overlay** — notes, ratings, and cook logs — to
whoever can reach the port. This page explains how to expose it *safely*.

fond stays local-first throughout: there is no fond account, no mandatory cloud,
and every option here is opt-in and self-hosted.

## Threat model

**What fond's built-in protection does:**

- **Refuses to start unprotected.** Binding to any non-loopback address (e.g.
  `0.0.0.0` or a LAN IP) without a configured token makes `fond serve` exit with
  an error, so you can't accidentally expose an open instance.
- **HTTP Basic Auth gate.** When `FOND_AUTH_TOKEN` (or `--auth-token`) is set,
  every request must present the shared secret as the Basic Auth password. The
  comparison is constant-time. Any username is accepted; the token is the
  password.
- **Optional native TLS.** With `--tls-cert`/`--tls-key`, fond terminates HTTPS
  itself using rustls.

**What fond deliberately does *not* do:**

- **No user accounts, roles, or per-user permissions.** There is a single shared
  secret for the whole household. Everyone who has it sees everything.
- **No rate limiting, brute-force lockout, or audit log.** Keep the token strong
  and the instance off the public internet.
- **No protection for a token sent in cleartext.** HTTP Basic Auth transmits the
  token on **every request**, and Base64 is an encoding, **not** encryption — over
  plain HTTP the token travels in the clear and anyone who can observe the network
  can read it. **Always pair `--auth-token` with TLS or a VPN** (native TLS, a
  reverse proxy, or Tailscale/WireGuard). fond warns you if you enable auth on a
  non-loopback bind without TLS.
- **A generic `401` challenge.** An unauthenticated request gets the standard
  Basic Auth challenge; the response is identical whether credentials were missing
  or wrong, and never reveals the token or hints at how auth is configured.
- **It is not a hardened public endpoint.** Do not port-forward fond straight to
  the internet. Use a VPN or an authenticated reverse proxy.

## Recommended stack

For a household, the simplest robust setup — in order of preference:

1. **Private network (VPN) first.** Put every device on a private overlay network
   with [Tailscale](https://tailscale.com) or [WireGuard](https://www.wireguard.com).
   The UI is then only reachable by your own devices and never touches the public
   internet. This alone removes most of the risk.
2. **Reverse proxy for TLS + auth (recommended for HTTPS).** Terminate TLS at
   [Caddy](https://caddyserver.com) (automatic certificates), nginx, or Traefik.
   Bind fond to loopback and let the proxy face the network.
3. **Shared token defense-in-depth.** Set `FOND_AUTH_TOKEN` so fond enforces auth
   even behind the proxy/VPN.

You don't need all three, but each layer is cheap. VPN + token is a great
baseline; add a reverse proxy when you want real HTTPS in the browser.

### Option A — Reverse proxy with Caddy (recommended)

Bind fond to loopback and let Caddy handle TLS and forward requests. fond still
enforces the token, so a misconfigured proxy can't expose an open instance.

Run fond (loopback, with a token):

```bash
export FOND_AUTH_TOKEN="$(openssl rand -base64 24)"
fond serve --bind 127.0.0.1 --port 3000
```

`Caddyfile`:

```caddy
recipes.example.com {
    reverse_proxy 127.0.0.1:3000
}
```

Caddy provisions and renews a TLS certificate automatically. Household members
visit `https://recipes.example.com`, and the browser prompts once for the
username (anything) and password (your token).

> Prefer the proxy to do the auth prompt? Caddy's
> [`basic_auth`](https://caddyserver.com/docs/caddyfile/directives/basic_auth)
> directive works too. Keeping the token on fond as well is belt-and-suspenders.

### Option B — Native TLS in fond (VPN-only setups)

If you don't want a separate proxy — for example, everything is already on
Tailscale — fond can terminate TLS itself. Supply a PEM certificate and key:

```bash
export FOND_AUTH_TOKEN="$(openssl rand -base64 24)"
fond serve \
  --bind 0.0.0.0 --port 3000 \
  --tls-cert /path/to/cert.pem \
  --tls-key  /path/to/key.pem \
  --auth-token "$FOND_AUTH_TOKEN"
```

For a Tailscale network you can mint a real, trusted certificate with
`tailscale cert <machine>.<tailnet>.ts.net`, which writes a `.crt`/`.key` pair to
pass to `--tls-cert`/`--tls-key`. A self-signed certificate also works but every
browser will warn on first visit.

## The safe-by-default guard

fond will not silently expose an open instance:

| Bind | Auth token | Result |
|---|---|---|
| `127.0.0.1` (loopback) | none | Starts. No auth needed. |
| `0.0.0.0` / LAN IP | set | Starts. Basic Auth enforced. |
| `0.0.0.0` / LAN IP | **none** | **Refuses to start.** |
| `0.0.0.0` / LAN IP | none + `--insecure-allow-no-auth` | Starts with a loud warning. |

`--insecure-allow-no-auth` exists only for the case where you have *already*
isolated the network yourself (e.g. a trusted air-gapped LAN) and accept the
risk. If you're unsure, don't use it — set a token instead.

If you enable auth on a non-loopback bind but don't configure TLS, fond warns
that credentials will travel in cleartext. Fix it by adding TLS (Option A or B).

## Generating a token

Any high-entropy string works. A quick one:

```bash
openssl rand -base64 24
```

Store it in your process manager / shell profile as `FOND_AUTH_TOKEN` rather than
passing `--auth-token` on the command line (where it can leak into shell
history and process listings).
