#!/usr/bin/env python3
"""LinkedIn OAuth2 Token Helper for OpenFang.

Automates the OAuth 2.0 Authorization Code flow to obtain a
LINKEDIN_ACCESS_TOKEN for the LinkedIn Hand.

Usage:
    python3 scripts/linkedin-oauth.py
    python3 scripts/linkedin-oauth.py --client-id ID --client-secret SECRET
    python3 scripts/linkedin-oauth.py --port 9090

Prerequisites:
    1. Create a LinkedIn Developer App at https://www.linkedin.com/developers/apps
    2. Under Auth tab, add redirect URL: http://localhost:8080/callback
    3. Request products: "Share on LinkedIn" and "Sign In with LinkedIn using OpenID Connect"
"""

import argparse
import http.server
import json
import os
import secrets
import sys
import threading
import urllib.error
import urllib.parse
import urllib.request
import webbrowser
from getpass import getpass

SCOPES = "openid profile email w_member_social"
AUTH_URL = "https://www.linkedin.com/oauth/v2/authorization"
TOKEN_URL = "https://www.linkedin.com/oauth/v2/accessToken"
USERINFO_URL = "https://api.linkedin.com/v2/userinfo"
TIMEOUT_SECONDS = 120


def exchange_code(code, client_id, client_secret, redirect_uri):
    """Exchange authorization code for access token."""
    data = urllib.parse.urlencode({
        "grant_type": "authorization_code",
        "code": code,
        "redirect_uri": redirect_uri,
        "client_id": client_id,
        "client_secret": client_secret,
    }).encode()

    req = urllib.request.Request(TOKEN_URL, data=data, method="POST")
    req.add_header("Content-Type", "application/x-www-form-urlencoded")

    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        print(f"\n  Token exchange failed (HTTP {e.code}): {body}", file=sys.stderr)
        sys.exit(1)
    except urllib.error.URLError as e:
        print(f"\n  Token exchange failed: {e.reason}", file=sys.stderr)
        sys.exit(1)


def validate_token(access_token):
    """Validate token by fetching user profile."""
    req = urllib.request.Request(USERINFO_URL)
    req.add_header("Authorization", f"Bearer {access_token}")

    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        print(f"\n  Token validation failed (HTTP {e.code}): {body}", file=sys.stderr)
        return None
    except urllib.error.URLError as e:
        print(f"\n  Token validation failed: {e.reason}", file=sys.stderr)
        return None


class OAuthCallbackHandler(http.server.BaseHTTPRequestHandler):
    """HTTP handler that captures the OAuth callback."""

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path != "/callback":
            self.send_response(404)
            self.end_headers()
            return

        params = urllib.parse.parse_qs(parsed.query)

        if "error" in params:
            self.send_response(400)
            self.send_header("Content-Type", "text/html")
            self.end_headers()
            error = params["error"][0]
            desc = params.get("error_description", ["Unknown error"])[0]
            self.wfile.write(f"<h2>Authorization Failed</h2><p>{error}: {desc}</p>".encode())
            self.server.oauth_error = f"{error}: {desc}"
            return

        code = params.get("code", [None])[0]
        state = params.get("state", [None])[0]

        if not code:
            self.send_response(400)
            self.send_header("Content-Type", "text/html")
            self.end_headers()
            self.wfile.write(b"<h2>Missing authorization code</h2>")
            self.server.oauth_error = "Missing authorization code"
            return

        if state != self.server.expected_state:
            self.send_response(400)
            self.send_header("Content-Type", "text/html")
            self.end_headers()
            self.wfile.write(b"<h2>State mismatch - possible CSRF attack</h2>")
            self.server.oauth_error = "State mismatch"
            return

        self.send_response(200)
        self.send_header("Content-Type", "text/html")
        self.end_headers()
        self.wfile.write(
            b"<h2>Authorization successful!</h2>"
            b"<p>You can close this tab and return to the terminal.</p>"
        )
        self.server.oauth_code = code

    def log_message(self, format, *args):
        """Suppress default HTTP logging."""
        pass


def run_oauth_flow(client_id, client_secret, port):
    """Run the full OAuth 2.0 Authorization Code flow."""
    redirect_uri = f"http://localhost:{port}/callback"
    state = secrets.token_hex(16)

    # Build authorization URL
    auth_params = urllib.parse.urlencode({
        "response_type": "code",
        "client_id": client_id,
        "redirect_uri": redirect_uri,
        "scope": SCOPES,
        "state": state,
    })
    auth_url = f"{AUTH_URL}?{auth_params}"

    # Start callback server
    server = http.server.HTTPServer(("localhost", port), OAuthCallbackHandler)
    server.timeout = TIMEOUT_SECONDS
    server.oauth_code = None
    server.oauth_error = None
    server.expected_state = state

    print(f"\n  Opening browser for LinkedIn authorization...")
    print(f"  Waiting for callback on http://localhost:{port}/callback ...")
    print(f"  (timeout: {TIMEOUT_SECONDS}s)\n")

    # Open browser in background
    webbrowser.open(auth_url)

    # Wait for callback (blocking with timeout)
    while server.oauth_code is None and server.oauth_error is None:
        server.handle_request()

    server.server_close()

    if server.oauth_error:
        print(f"  Authorization failed: {server.oauth_error}", file=sys.stderr)
        sys.exit(1)

    if not server.oauth_code:
        print("  Timeout — no authorization received.", file=sys.stderr)
        sys.exit(1)

    print("  Authorization code received")

    # Exchange code for token
    token_data = exchange_code(server.oauth_code, client_id, client_secret, redirect_uri)
    access_token = token_data.get("access_token")
    expires_in = token_data.get("expires_in", 0)

    if not access_token:
        print(f"  No access_token in response: {token_data}", file=sys.stderr)
        sys.exit(1)

    print("  Token exchanged successfully")

    # Validate token
    profile = validate_token(access_token)
    if profile:
        name = profile.get("name", "Unknown")
        email = profile.get("email", "")
        sub = profile.get("sub", "")
        email_display = f" ({email})" if email else ""
        print(f"  Token validated — Hello, {name}{email_display}")
        if sub:
            print(f"  Member URN: urn:li:person:{sub}")
    else:
        print("  Token obtained but validation failed — token may still work")

    # Output
    days = expires_in // 86400
    print(f"\n{'=' * 60}")
    print(f"  Your LinkedIn access token (expires in {days} days):\n")
    print(f"  export LINKEDIN_ACCESS_TOKEN={access_token}")
    print(f"\n{'=' * 60}")
    print(f"\n  Add to ~/.zshrc (or ~/.bashrc) then restart OpenFang.")
    print(f"  Or run the export command above before starting the daemon.\n")


def main():
    parser = argparse.ArgumentParser(description="LinkedIn OAuth2 Token Helper for OpenFang")
    parser.add_argument("--client-id", help="LinkedIn App Client ID")
    parser.add_argument("--client-secret", help="LinkedIn App Client Secret")
    parser.add_argument("--port", type=int, default=8080, help="Callback port (default: 8080)")
    args = parser.parse_args()

    print("\n  LinkedIn OAuth2 — Token Helper")
    print("  " + "=" * 40)
    print()
    print("  Prerequisites:")
    print("    1. LinkedIn Developer App at https://www.linkedin.com/developers/apps")
    print(f"    2. Redirect URL set to: http://localhost:{args.port}/callback")
    print("    3. Products enabled: 'Share on LinkedIn' + 'Sign In with LinkedIn using OpenID Connect'")
    print()

    client_id = args.client_id or input("  Enter Client ID: ").strip()
    client_secret = args.client_secret or getpass("  Enter Client Secret: ").strip()

    if not client_id or not client_secret:
        print("\n  Client ID and Client Secret are required.", file=sys.stderr)
        sys.exit(1)

    run_oauth_flow(client_id, client_secret, args.port)


if __name__ == "__main__":
    main()
