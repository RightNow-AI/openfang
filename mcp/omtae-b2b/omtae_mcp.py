#!/usr/bin/env python3
"""
OMTAE B2B Revenue Tools — stdio MCP server for Hermes Desktop.

Setup (uv):
    uv venv .venv && source .venv/bin/activate
    uv pip install -r requirements.txt
    python omtae_mcp.py

Setup (pip):
    python3 -m venv .venv && source .venv/bin/activate
    pip install -r requirements.txt
    python omtae_mcp.py

Hermes Desktop MCP config example:
    {
      "mcpServers": {
        "omtae-b2b": {
          "command": "/path/to/.venv/bin/python",
          "args": ["/path/to/omtae_mcp.py"]
        }
      }
    }
"""

from __future__ import annotations

import hashlib
import json
import re
import time
from typing import Any
from urllib.parse import urljoin, urlparse

import httpx
from bs4 import BeautifulSoup
from mcp.server.fastmcp import FastMCP

mcp = FastMCP("omtae-b2b")

USER_AGENT = "OMTAE-B2B-Auditor/1.0 (+https://omtaeservices.biz)"
REQUEST_TIMEOUT = 20.0

CHAT_WIDGET_SIGNATURES: tuple[str, ...] = (
    "intercom",
    "drift.com",
    "drift.load",
    "tidio",
    "crisp.chat",
    "client.crisp.chat",
    "hubspot",
    "hs-scripts.com",
    "hs-beacon",
    "zendesk",
    "zdassets.com",
    "zopim",
    "livechatinc.com",
    "tawk.to",
    "olark",
    "freshchat",
    "freshworks.com",
    "liveperson",
    "ada.support",
    "genesys",
    "embeddedservice",
    "salesforce.com/embeddedservice",
    "widget.intercom.io",
    "chatbot.com",
    "botpress",
    "manychat",
)

NICHE_SUFFIXES: tuple[str, ...] = (
    "Group",
    "Partners",
    "Associates",
    "Solutions",
    "Services",
    "Consulting",
    "Studio",
    "Co.",
)

NICHE_PREFIXES: tuple[str, ...] = (
    "Summit",
    "Pioneer",
    "Atlas",
    "Horizon",
    "Vertex",
    "Cedar",
    "Lone Star",
    "Metro",
)


def _slug(text: str) -> str:
    return re.sub(r"[^a-z0-9]+", "", text.lower())


def _seed(*parts: str) -> int:
    digest = hashlib.sha256("|".join(parts).encode()).hexdigest()
    return int(digest[:8], 16)


def _normalize_domain(domain: str) -> str:
    domain = domain.strip()
    if not domain:
        raise ValueError("domain must not be empty")
    if "://" in domain:
        parsed = urlparse(domain)
        host = parsed.netloc or parsed.path
    else:
        host = domain.split("/")[0]
    host = host.removeprefix("www.")
    return f"https://{host}"


def _extract_contact_links(soup: BeautifulSoup) -> dict[str, list[str]]:
    mailto: list[str] = []
    tel: list[str] = []
    for anchor in soup.find_all("a", href=True):
        href = anchor["href"].strip()
        if href.lower().startswith("mailto:"):
            mailto.append(href)
        elif href.lower().startswith("tel:"):
            tel.append(href)
    return {"mailto": mailto, "tel": tel}


def _detect_chat_widgets(soup: BeautifulSoup, html: str) -> dict[str, Any]:
    haystack = html.lower()
    script_sources = [
        tag.get("src", "")
        for tag in soup.find_all("script")
        if tag.get("src")
    ]
    iframe_sources = [
        tag.get("src", "")
        for tag in soup.find_all("iframe")
        if tag.get("src")
    ]
    combined = "\n".join([haystack, *script_sources, *iframe_sources])

    detected: list[str] = []
    for signature in CHAT_WIDGET_SIGNATURES:
        if signature in combined:
            detected.append(signature)

    # De-duplicate while preserving order.
    seen: set[str] = set()
    unique = []
    for item in detected:
        if item not in seen:
            seen.add(item)
            unique.append(item)

    return {
        "detected": unique,
        "has_chat_widget": len(unique) > 0,
        "script_count": len(script_sources),
        "iframe_count": len(iframe_sources),
    }


def _mock_phone(location: str, index: int) -> str:
    area_codes = {
        "texas": "512",
        "austin": "512",
        "dallas": "214",
        "houston": "713",
        "san antonio": "210",
    }
    loc = location.lower()
    area = next((code for key, code in area_codes.items() if key in loc), "555")
    return f"({area}) 555-{1000 + index:04d}"


@mcp.tool()
def discover_niche_targets(niche: str, location: str) -> list[dict[str, str]]:
    """Discover local B2B targets for a niche and geography (mocked prospect list).

    Args:
        niche: Industry or service vertical (e.g. "dental clinics", "HVAC contractors").
        location: City/region to search (e.g. "Austin, TX").

    Returns:
        JSON-serializable list of 3-5 businesses with company_name, domain, and phone.
    """
    niche = niche.strip() or "local businesses"
    location = location.strip() or "your market"
    seed = _seed(niche, location)
    count = 3 + (seed % 3)  # 3-5 targets

    niche_slug = _slug(niche) or "business"
    location_slug = _slug(location.split(",")[0]) or "local"

    targets: list[dict[str, str]] = []
    for i in range(count):
        prefix = NICHE_PREFIXES[(seed + i) % len(NICHE_PREFIXES)]
        suffix = NICHE_SUFFIXES[(seed // (i + 1) + i) % len(NICHE_SUFFIXES)]
        company_name = f"{prefix} {niche.title()} {suffix}"
        domain = f"https://www.{location_slug}-{niche_slug}-{i + 1}.example"
        targets.append(
            {
                "company_name": company_name,
                "domain": domain,
                "phone": _mock_phone(location, i),
            }
        )

    return targets


@mcp.tool()
def analyze_site_infrastructure(domain: str) -> dict[str, Any]:
    """Audit a company website for B2B AI integration opportunities.

    Fetches the homepage, inspects DOM contact patterns, detects chat widgets,
    and records response timing and headers.

    Args:
        domain: Root domain or full URL (e.g. "acmehvac.com").

    Returns:
        JSON dictionary of technical vulnerabilities and AI integration opportunities.
    """
    url = _normalize_domain(domain)
    started = time.perf_counter()

    try:
        with httpx.Client(
            timeout=REQUEST_TIMEOUT,
            follow_redirects=True,
            headers={"User-Agent": USER_AGENT},
        ) as client:
            response = client.get(url)
    except httpx.HTTPError as exc:
        return {
            "domain": domain,
            "url": url,
            "reachable": False,
            "error": str(exc),
            "vulnerabilities": ["site_unreachable"],
            "opportunities": ["deploy_ai_front_door_capture_for_unreachable_site"],
        }

    elapsed_ms = round((time.perf_counter() - started) * 1000, 2)
    html = response.text
    soup = BeautifulSoup(html, "html.parser")

    contacts = _extract_contact_links(soup)
    widgets = _detect_chat_widgets(soup, html)

    has_mailto = len(contacts["mailto"]) > 0
    has_tel = len(contacts["tel"]) > 0
    has_raw_contact = has_mailto or has_tel
    no_chat_widget = not widgets["has_chat_widget"]

    vulnerabilities: list[str] = []
    opportunities: list[str] = []

    if response.status_code >= 400:
        vulnerabilities.append(f"http_status_{response.status_code}")
    if elapsed_ms > 3000:
        vulnerabilities.append("slow_page_load")
    if has_raw_contact and no_chat_widget:
        vulnerabilities.append("raw_contact_links_without_ai_chat")
        opportunities.append("replace_static_mailto_tel_with_24_7_ai_concierge")
    if no_chat_widget:
        opportunities.append("add_conversational_lead_capture_widget")
    if not response.headers.get("strict-transport-security"):
        vulnerabilities.append("missing_hsts_header")
    if "x-powered-by" in {k.lower() for k in response.headers.keys()}:
        vulnerabilities.append("technology_fingerprint_exposed")

    title = (soup.title.string or "").strip() if soup.title else ""
    forms = len(soup.find_all("form"))

    return {
        "domain": domain,
        "url": str(response.url),
        "reachable": True,
        "status_code": response.status_code,
        "page_title": title,
        "load_time_ms": elapsed_ms,
        "response_headers": {
            "server": response.headers.get("server"),
            "content_type": response.headers.get("content-type"),
            "cache_control": response.headers.get("cache-control"),
        },
        "contact_links": contacts,
        "chat_widgets": widgets,
        "form_count": forms,
        "vulnerabilities": vulnerabilities,
        "opportunities": opportunities,
        "ai_readiness_score": max(0, 100 - (20 * len(vulnerabilities)) + (10 * len(opportunities))),
    }


@mcp.tool()
def generate_audit_report(company_name: str, audit_data: dict[str, Any]) -> str:
    """Generate a hyper-targeted Markdown pitch from raw infrastructure audit data.

    Args:
        company_name: Prospect company name.
        audit_data: Output dictionary from analyze_site_infrastructure.

    Returns:
        Markdown report proposing a custom AI integration plan.
    """
    company_name = company_name.strip() or "Prospect"
    data = audit_data if isinstance(audit_data, dict) else {}

    domain = data.get("domain", "unknown-domain")
    url = data.get("url", domain)
    vulnerabilities = data.get("vulnerabilities", [])
    opportunities = data.get("opportunities", [])
    score = data.get("ai_readiness_score", "n/a")
    load_time = data.get("load_time_ms", "n/a")
    widgets = data.get("chat_widgets", {})
    contacts = data.get("contact_links", {})

    vuln_lines = "\n".join(f"- {item.replace('_', ' ')}" for item in vulnerabilities) or "- None observed"
    opp_lines = "\n".join(f"- {item.replace('_', ' ')}" for item in opportunities) or "- General AI front-office uplift"

    mailto_count = len(contacts.get("mailto", []))
    tel_count = len(contacts.get("tel", []))
    chat_detected = widgets.get("detected", [])

    primary_pitch = (
        "Deploy an OMTAE-managed AI concierge that captures inbound intent 24/7, "
        "routes qualified leads to your CRM, and replaces static contact links with "
        "guided conversion flows."
    )
    if "raw_contact_links_without_ai_chat" in vulnerabilities:
        primary_pitch = (
            f"We found {mailto_count} mailto and {tel_count} tel links with no conversational layer. "
            "OMTAE can deploy a branded AI intake agent that qualifies visitors before handoff."
        )

    report = f"""# OMTAE Technical Audit — {company_name}

## Executive Summary
{company_name} ({url}) is a strong candidate for a custom AI integration package.
Current AI readiness score: **{score}/100**.

## What We Found
### Technical Vulnerabilities
{vuln_lines}

### AI Integration Opportunities
{opp_lines}

## Infrastructure Snapshot
- Page load time: **{load_time} ms**
- Conversational widgets detected: **{', '.join(chat_detected) if chat_detected else 'None'}**
- Static contact endpoints: **{mailto_count} mailto / {tel_count} tel**

## Recommended OMTAE Solution
{primary_pitch}

### 30-Day Rollout
1. **Week 1:** Install OMTAE desk agent + website concierge widget on `{domain}`.
2. **Week 2:** Connect lead routing to your CRM and call-back workflow.
3. **Week 3:** Add niche-specific qualification prompts and objection handling.
4. **Week 4:** Launch analytics dashboard for captured intents and conversion lift.

## Expected Business Impact
- Faster response to inbound leads (seconds vs hours)
- Higher qualified meeting rate from after-hours traffic
- Lower dependency on static `mailto:`/`tel:` links that lose context

## Next Step
Approve a pilot and OMTAE will deploy a production agent wired to your existing site in under one week.
"""
    return report


def main() -> None:
    mcp.run(transport="stdio")


if __name__ == "__main__":
    main()
