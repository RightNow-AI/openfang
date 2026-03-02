---
name: recon-hand-skill
version: "1.0.0"
description: "Expert knowledge for security reconnaissance — ProjectDiscovery toolchain mastery, attack surface methodology, vulnerability classification, and continuous asset monitoring"
runtime: prompt_only
---

# Security Reconnaissance Expert Knowledge

## ProjectDiscovery Ecosystem Overview

The ProjectDiscovery ecosystem is a suite of open-source security tools designed to work together as a pipeline. Each tool handles one phase of reconnaissance.

| Tool | Purpose | Input | Output |
|------|---------|-------|--------|
| **subfinder** | Passive subdomain enumeration | Root domain | Subdomain list |
| **dnsx** | DNS resolution and validation | Subdomain list | Resolved hosts with IPs |
| **naabu** | Port scanning | Host list | Host:port pairs |
| **httpx** | HTTP probing and tech detection | Host list | Live web services with metadata |
| **nuclei** | Vulnerability scanning | URL list | Vulnerability findings |
| **katana** | Web crawling/spidering | URL list | Discovered endpoints |
| **uncover** | Search engine dorking | Query | Exposed assets |

### Standard Pipeline

```
subfinder → dnsx → naabu → httpx → nuclei
   ↓          ↓       ↓        ↓        ↓
subdomains  resolved  ports    live     vulns
            hosts     open     services found
```

---

## Subfinder — Subdomain Discovery

### How It Works

Subfinder uses passive sources only — it never touches the target directly. Sources include:
- Certificate Transparency logs (crt.sh, Certspotter)
- Search engines (Google, Bing, Yahoo, Baidu)
- DNS datasets (DNSdumpster, RapidDNS, Riddler)
- Security platforms (Shodan, Censys, VirusTotal, SecurityTrails)
- Archive services (Wayback Machine, Common Crawl)

### Key Flags

```bash
# Basic enumeration
subfinder -d example.com -silent

# Use all sources (slower but more thorough)
subfinder -d example.com -all -silent

# Multiple domains from file
subfinder -dL domains.txt -all -silent

# Output to file
subfinder -d example.com -all -silent -o subdomains.txt

# Exclude specific sources
subfinder -d example.com -es github -silent

# Show source for each subdomain
subfinder -d example.com -silent -cs
```

### API Keys for Better Results

Configure in `~/.config/subfinder/provider-config.yaml`:
```yaml
shodan:
  - SHODAN_API_KEY
censys:
  - CENSYS_API_ID:CENSYS_API_SECRET
securitytrails:
  - SECURITYTRAILS_API_KEY
virustotal:
  - VIRUSTOTAL_API_KEY
chaos:
  - PDCP_API_KEY
```

---

## dnsx — DNS Toolkit

### Key Flags

```bash
# Resolve A records
cat subdomains.txt | dnsx -silent -a -resp

# Resolve multiple record types
cat subdomains.txt | dnsx -silent -a -aaaa -cname -mx -ns -txt -resp

# Filter by response code
cat subdomains.txt | dnsx -silent -a -resp -rc noerror

# Wildcard detection and filtering
cat subdomains.txt | dnsx -silent -a -resp -wd example.com

# Reverse DNS
echo "1.2.3.4" | dnsx -silent -ptr

# JSON output
cat subdomains.txt | dnsx -silent -a -resp -json -o dns_results.json
```

### Subdomain Takeover Detection via CNAME

Vulnerable CNAME targets (service no longer claimed):

| CNAME Pattern | Service | Takeover Method |
|--------------|---------|----------------|
| `*.s3.amazonaws.com` | AWS S3 | Create bucket with matching name |
| `*.azurewebsites.net` | Azure | Create App Service with matching name |
| `*.herokuapp.com` | Heroku | Create app with matching name |
| `*.github.io` | GitHub Pages | Create repo with matching name |
| `*.cloudfront.net` | AWS CloudFront | Create distribution |
| `*.shopify.com` | Shopify | Claim via Shopify admin |
| `*.ghost.io` | Ghost | Create Ghost blog |
| `*.netlify.app` | Netlify | Create Netlify site |
| `*.vercel.app` | Vercel | Create Vercel project |
| `*.surge.sh` | Surge | Deploy to matching subdomain |

Detection: CNAME exists but target returns NXDOMAIN or service-specific error page.

---

## Naabu — Port Scanner

### Key Flags

```bash
# Top 100 ports (fast)
naabu -host example.com -top-ports 100 -silent

# Top 1000 ports (standard)
cat hosts.txt | naabu -top-ports 1000 -silent

# Full port scan
cat hosts.txt | naabu -p - -silent

# Specific ports
naabu -host example.com -p 80,443,8080,8443 -silent

# Rate limiting
cat hosts.txt | naabu -top-ports 1000 -rate 100 -silent

# With service detection
cat hosts.txt | naabu -top-ports 1000 -silent -sn

# JSON output
cat hosts.txt | naabu -top-ports 1000 -silent -json -o ports.json

# Exclude CDN IPs
cat hosts.txt | naabu -top-ports 1000 -silent -ec
```

### High-Value Ports

| Port | Service | Security Relevance |
|------|---------|-------------------|
| 21 | FTP | Anonymous access, cleartext creds |
| 22 | SSH | Brute force target, version info |
| 23 | Telnet | Cleartext, often default creds |
| 25 | SMTP | Open relay, email spoofing |
| 53 | DNS | Zone transfer, cache poisoning |
| 80/443 | HTTP/HTTPS | Web application attacks |
| 110/143 | POP3/IMAP | Email access, cleartext |
| 445 | SMB | EternalBlue, share enumeration |
| 1433 | MSSQL | Default creds, SQLi |
| 3306 | MySQL | Default creds, exposed DB |
| 3389 | RDP | Brute force, BlueKeep |
| 5432 | PostgreSQL | Default creds, exposed DB |
| 5900 | VNC | Weak/no auth |
| 6379 | Redis | Unauthenticated access |
| 8080/8443 | HTTP Alt | Dev servers, admin panels |
| 9200 | Elasticsearch | Unauthenticated access |
| 27017 | MongoDB | Unauthenticated access |

---

## httpx — HTTP Toolkit

### Key Flags

```bash
# Basic probing with metadata
cat hosts.txt | httpx -silent -title -status-code -content-length

# Technology detection
cat hosts.txt | httpx -silent -tech-detect

# Full metadata extraction
cat hosts.txt | httpx -silent -title -status-code -tech-detect \
  -content-length -web-server -cdn -follow-redirects

# TLS certificate info
cat hosts.txt | httpx -silent -tls-grab -tls-probe

# Screenshot (requires chromium)
cat hosts.txt | httpx -silent -screenshot

# Filter by status code
cat hosts.txt | httpx -silent -mc 200,301,302,403

# JSON output (recommended for parsing)
cat hosts.txt | httpx -silent -json \
  -title -status-code -tech-detect -content-length \
  -web-server -cdn -tls-grab -follow-redirects \
  -o http_results.json

# Custom headers
cat hosts.txt | httpx -silent -H "User-Agent: Mozilla/5.0"

# Response body hash (for change detection)
cat hosts.txt | httpx -silent -hash md5
```

### Technology Fingerprinting

httpx detects technologies via Wappalyzer signatures. Key categories:
- **Web frameworks**: React, Angular, Vue, Django, Rails, Laravel, Spring
- **Web servers**: nginx, Apache, IIS, Caddy, LiteSpeed
- **CMS**: WordPress, Drupal, Joomla, Ghost
- **CDN**: Cloudflare, AWS CloudFront, Akamai, Fastly
- **WAF**: Cloudflare, AWS WAF, Imperva, ModSecurity
- **Languages**: PHP, Python, Ruby, Java, Node.js, Go
- **Analytics**: Google Analytics, Matomo, Mixpanel

---

## Nuclei — Vulnerability Scanner

### Template System

Nuclei templates are YAML files that define detection logic. Community-maintained at:
`github.com/projectdiscovery/nuclei-templates`

Template categories:
```
nuclei-templates/
├── cves/                  # Known CVE detections
├── vulnerabilities/       # Generic vulnerability checks
├── misconfiguration/      # Server/service misconfigs
├── exposures/             # Sensitive file/panel exposure
│   ├── configs/           # Exposed config files
│   ├── panels/            # Admin panels
│   └── tokens/            # API keys, secrets
├── technologies/          # Technology detection
├── default-logins/        # Default credential checks
├── takeovers/             # Subdomain takeover detection
├── file/                  # Local file analysis
├── dns/                   # DNS-based checks
├── ssl/                   # TLS/SSL checks
└── headless/              # Browser-based checks
```

### Key Flags

```bash
# Standard scan with severity filter
cat urls.txt | nuclei -severity medium,high,critical -silent

# Specific template categories
cat urls.txt | nuclei -tags cves,misconfiguration -silent

# Specific templates
nuclei -u https://example.com -t cves/2024/ -silent

# Rate limiting
cat urls.txt | nuclei -rate-limit 50 -silent

# Bulk rate limiting
cat urls.txt | nuclei -bulk-size 25 -concurrency 10 -silent

# JSON output
cat urls.txt | nuclei -severity medium,high,critical -json -o findings.json

# Exclude certain templates
cat urls.txt | nuclei -etags dos,fuzz -silent

# New templates only (added in last update)
cat urls.txt | nuclei -new-templates -silent

# Automatic scan (smart template selection)
cat urls.txt | nuclei -as -silent

# Headless mode (browser-based detection)
cat urls.txt | nuclei -headless -silent

# Update templates
nuclei -update-templates
```

### Severity Classification

| Severity | Description | Examples |
|----------|------------|---------|
| **Critical** | Direct exploitation possible, high impact | RCE, SQLi, auth bypass, SSRF to internal |
| **High** | Significant security impact | Stored XSS, path traversal, IDOR, sensitive data exposure |
| **Medium** | Moderate impact, may need chaining | Reflected XSS, CSRF, open redirect, info disclosure |
| **Low** | Minor security concern | Missing headers, version disclosure, deprecated ciphers |
| **Info** | Informational, no direct security impact | Technology detection, CDN/WAF identification |

### Custom Template Writing

```yaml
id: custom-check-example
info:
  name: Example Custom Check
  author: recon-hand
  severity: medium
  description: Detects exposed configuration file
  tags: exposure,config

http:
  - method: GET
    path:
      - "{{BaseURL}}/.env"
      - "{{BaseURL}}/config.yml"
    matchers-condition: and
    matchers:
      - type: status
        status:
          - 200
      - type: word
        words:
          - "DB_PASSWORD"
          - "API_KEY"
          - "SECRET"
        condition: or
```

---

## Attack Surface Methodology

### Reconnaissance Phases

```
Phase 1: PASSIVE ENUMERATION
├── Subdomain discovery (subfinder)
├── Certificate transparency search
├── DNS record enumeration
├── WHOIS and registrar info
├── Historical data (Wayback Machine)
└── Search engine dorking

Phase 2: ACTIVE VALIDATION
├── DNS resolution (dnsx)
├── Port scanning (naabu)
├── HTTP probing (httpx)
├── Technology fingerprinting
└── TLS certificate analysis

Phase 3: VULNERABILITY DETECTION
├── Known CVE scanning (nuclei)
├── Misconfiguration detection
├── Default credential checking
├── Exposed panel/file detection
└── Subdomain takeover verification

Phase 4: ANALYSIS & REPORTING
├── Asset inventory
├── Risk scoring
├── Change detection (delta)
├── Remediation priorities
└── Executive summary
```

### Risk Scoring Matrix

| Factor | Weight | Low (1) | Medium (2) | High (3) |
|--------|--------|---------|-----------|----------|
| Severity | 40% | Info/Low finding | Medium finding | High/Critical finding |
| Exposure | 25% | Internal only | Partially exposed | Internet-facing |
| Exploitability | 20% | Complex/theoretical | Requires conditions | Trivially exploitable |
| Data sensitivity | 15% | Public data | Internal data | PII/credentials/financial |

```
Risk Score = (Severity × 0.4) + (Exposure × 0.25) + (Exploitability × 0.2) + (Data × 0.15)
Risk Level: 1.0-1.5 = Low, 1.5-2.0 = Medium, 2.0-2.5 = High, 2.5-3.0 = Critical
```

---

## Change Detection Patterns

### Asset Delta Analysis

On each scan cycle, compare against previous baseline:

| Change Type | Detection Method | Significance |
|-------------|-----------------|-------------|
| New subdomain | Set difference on subfinder output | Medium — could be shadow IT or attacker |
| Removed subdomain | Set difference (reverse) | Low — decommissioned service |
| New open port | Set difference on naabu output | High — unexpected service exposure |
| Closed port | Set difference (reverse) | Low — service hardened |
| New technology | httpx tech-detect diff | Medium — new attack surface |
| New vulnerability | nuclei output diff | High/Critical — immediate attention |
| Certificate change | TLS grab comparison | Medium — renewal or compromise |
| IP address change | DNS resolution diff | Medium — infrastructure change |

### Baseline Management

```bash
# Save current scan as baseline
cp recon_output/subdomains/domain_raw.txt recon_output/subdomains/domain_previous.txt
cp recon_output/http/domain_http.json recon_output/http/domain_http_previous.json
cp recon_output/vulnerabilities/domain_nuclei.json recon_output/vulnerabilities/domain_nuclei_previous.json

# Compute deltas
comm -13 <(sort previous.txt) <(sort current.txt) > new_items.txt
comm -23 <(sort previous.txt) <(sort current.txt) > removed_items.txt
```

---

## Common Vulnerability Patterns

### Web Application

| Pattern | Detection | Nuclei Tags |
|---------|----------|-------------|
| Exposed `.env` files | `nuclei -tags exposure,config` | exposures/configs |
| Open admin panels | `nuclei -tags panel` | exposures/panels |
| Default credentials | `nuclei -tags default-login` | default-logins |
| Outdated software (CVEs) | `nuclei -tags cves` | cves |
| Missing security headers | `nuclei -tags headers` | misconfiguration |
| Open redirects | `nuclei -tags redirect` | vulnerabilities |
| CORS misconfiguration | `nuclei -tags cors` | misconfiguration |
| Directory listing | `nuclei -tags listing` | misconfiguration |

### Infrastructure

| Pattern | Detection | Impact |
|---------|----------|--------|
| Unauthenticated Redis | Port 6379 + no AUTH | Data theft, RCE |
| Exposed Elasticsearch | Port 9200 + no auth | Data theft |
| Open MongoDB | Port 27017 + no auth | Data theft |
| Kubernetes dashboard | Port 443 + /api/v1 | Cluster compromise |
| Docker API exposed | Port 2375/2376 | Host compromise |
| Jenkins unauthenticated | Port 8080 + /script | RCE |

---

## Responsible Disclosure

### Scope Discipline

- **Only scan explicitly authorized targets** — never expand scope without permission
- **Respect rate limits** — aggressive scanning can cause DoS
- **No exploitation** — detect and report only, never attempt to exploit
- **Redact sensitive data** — remove credentials, PII, and internal IPs from reports
- **Preserve evidence** — save raw output for verification but secure it appropriately

### Finding Verification

Before reporting a finding as confirmed:
1. Check for false positive indicators (WAF interference, honeypot signatures)
2. Verify the finding is reproducible (not a transient condition)
3. Confirm the severity matches the actual impact (not just template default)
4. Check if the finding is already known/acknowledged by the target
