# OMTAE dashboard access over Cloudflare Tunnel

This guide covers exposing the OMTAE web dashboard (default port **4200**) through a personal Cloudflare Tunnel (e.g. `desk.omtaeservices.biz`).

## Security model

**PIN auth is for personal tunnels only.** A 4–6 digit PIN stops casual visitors but is not strong cryptography. Anyone who can reach your tunnel URL can attempt brute force. Combine with:

- Cloudflare Access (recommended for production)
- A long, unique PIN (not `123456`)
- Tunnel hostname not shared publicly
- Rate limiting at the edge where possible

Do **not** rely on PIN-only auth for multi-tenant or internet-facing deployments.

## Configuration

Add to `~/.omtae/config.toml`:

```toml
[dashboard]
pin = "your-pin-here"   # 4–6 digits — change from any default
require_pin = true
```

When `require_pin = true` and `pin` is set:

- The top-level `api_key` / `OPENFANG_API_KEY` **Bearer** requirement is disabled for HTTP/WebSocket (PIN + session cookie instead).
- `/api/*` requires a valid `omtae_session` cookie (after PIN login) or `X-OMTAE-Pin` header.
- Public endpoints remain open: `/api/health`, static assets, `/api/auth/login`, `/api/auth/check`.

Restart the daemon after changing config:

```bash
cargo build --release -p omtae-cli
sudo systemctl restart omtae-daemon.service
```

## vLLM API key

Local **vLLM** does not need a wizard API key. Configure the model in `[default_model]` and set `VLLM_API_KEY` in **systemd** (or your vLLM service) if the proxy requires it—not in the dashboard wizard.

## Client behavior

1. Open the tunnel URL in a browser.
2. Enter the PIN on the unlock screen (mobile-friendly numeric keypad).
3. The server sets an HttpOnly `omtae_session` cookie (default 7-day TTL via `[auth].session_ttl_hours`).
4. Optional: scripts can send `X-OMTAE-Pin: <pin>` on API calls instead of Bearer tokens.

## Legacy options

| Method | Use case |
|--------|----------|
| `api_key` at root of config.toml | Machine-to-machine / scripts |
| `[auth]` username + `password_hash` | Stronger dashboard login (`omtae auth hash-password`) |
| `[dashboard]` PIN | Quick personal tunnel gate |

PIN mode takes precedence over Bearer when `[dashboard]` is active.
