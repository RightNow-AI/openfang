---
name: apitester-hand-skill
version: "1.0.0"
description: "Expert knowledge for API testing — HTTP methods, status codes, curl patterns, OWASP API Top 10, authentication testing, performance benchmarking, and report templates"
runtime: prompt_only
---

# API Testing Expert Knowledge

## curl Command Reference for API Testing

### Basic Requests
```bash
# GET with headers and timing
curl -s -w "\nHTTP %{http_code} | Time: %{time_total}s | Size: %{size_download}B" \
  -H "Accept: application/json" \
  "https://api.example.com/resource"

# POST with JSON body
curl -s -X POST "https://api.example.com/resource" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"key": "value"}'

# PUT (full update)
curl -s -X PUT "https://api.example.com/resource/123" \
  -H "Content-Type: application/json" \
  -d '{"key": "updated_value"}'

# PATCH (partial update)
curl -s -X PATCH "https://api.example.com/resource/123" \
  -H "Content-Type: application/json" \
  -d '{"key": "patched_value"}'

# DELETE
curl -s -X DELETE "https://api.example.com/resource/123" \
  -H "Authorization: Bearer $TOKEN"

# HEAD (headers only, no body)
curl -sI "https://api.example.com/resource"

# OPTIONS (check allowed methods and CORS)
curl -s -X OPTIONS -I "https://api.example.com/resource"
```

### Advanced curl Flags
```bash
# Detailed timing breakdown
curl -s -o /dev/null -w "
  DNS Lookup:    %{time_namelookup}s
  TCP Connect:   %{time_connect}s
  TLS Handshake: %{time_appconnect}s
  First Byte:    %{time_starttransfer}s
  Total Time:    %{time_total}s
  Download Size: %{size_download} bytes
  HTTP Code:     %{http_code}
" "https://api.example.com/endpoint"

# Follow redirects
curl -sL "https://api.example.com/old-endpoint"

# Include response headers in output
curl -si "https://api.example.com/endpoint"

# Send form data
curl -s -X POST "https://api.example.com/upload" \
  -F "file=@/path/to/file.pdf" \
  -F "description=test upload"

# Custom timeout
curl -s --connect-timeout 5 --max-time 30 "https://api.example.com/slow-endpoint"

# Ignore SSL cert errors (testing only)
curl -sk "https://self-signed.example.com/api"

# Verbose output for debugging
curl -v "https://api.example.com/endpoint" 2>&1
```

### Authentication Patterns
```bash
# Bearer token
curl -s -H "Authorization: Bearer eyJhbGciOi..." "https://api.example.com/protected"

# API key in header
curl -s -H "X-API-Key: your-api-key-here" "https://api.example.com/data"

# API key in query string
curl -s "https://api.example.com/data?api_key=your-key-here"

# Basic auth
curl -s -u "username:password" "https://api.example.com/protected"

# OAuth2 client credentials flow
curl -s -X POST "https://auth.example.com/oauth/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials&client_id=ID&client_secret=SECRET&scope=read"
```

---

## HTTP Status Codes Reference

### 2xx Success
| Code | Name | Meaning |
|------|------|---------|
| 200 | OK | Request succeeded, response body contains result |
| 201 | Created | Resource successfully created (POST) |
| 202 | Accepted | Request accepted for async processing |
| 204 | No Content | Success, no response body (common for DELETE) |

### 3xx Redirection
| Code | Name | Meaning |
|------|------|---------|
| 301 | Moved Permanently | Resource has a new permanent URL |
| 302 | Found | Temporary redirect |
| 304 | Not Modified | Resource unchanged since last request (caching) |

### 4xx Client Errors
| Code | Name | Meaning | Common Cause |
|------|------|---------|--------------|
| 400 | Bad Request | Malformed request or invalid input | Missing required fields, wrong types |
| 401 | Unauthorized | No valid authentication provided | Missing or expired token |
| 403 | Forbidden | Authenticated but not authorized | Insufficient permissions |
| 404 | Not Found | Resource does not exist | Wrong URL or deleted resource |
| 405 | Method Not Allowed | HTTP method not supported | Using POST on a GET-only endpoint |
| 409 | Conflict | Request conflicts with current state | Duplicate resource, version conflict |
| 413 | Payload Too Large | Request body exceeds server limit | File upload too big |
| 415 | Unsupported Media Type | Wrong Content-Type header | Sending form data to JSON endpoint |
| 422 | Unprocessable Entity | Valid syntax but semantic errors | Business rule validation failure |
| 429 | Too Many Requests | Rate limit exceeded | Too many requests in time window |

### 5xx Server Errors
| Code | Name | Meaning | Testing Implication |
|------|------|---------|---------------------|
| 500 | Internal Server Error | Unhandled server exception | Always a finding — server should never expose unhandled errors |
| 502 | Bad Gateway | Upstream server error | Infrastructure issue |
| 503 | Service Unavailable | Server overloaded or in maintenance | Capacity issue |
| 504 | Gateway Timeout | Upstream server timeout | Slow dependency |

---

## OWASP API Security Top 10 (2023)

### API1:2023 — Broken Object Level Authorization (BOLA)
**What**: User can access other users' objects by changing resource IDs.
**Test pattern**: Authenticate as User A, request User B's resources by ID. If 200 returned instead of 403, BOLA exists.
**Severity**: Critical
**Example**: `GET /api/orders/12345` returns Order belonging to different user.

### API2:2023 — Broken Authentication
**What**: Weak or missing authentication mechanisms.
**Test patterns**:
- Brute-force login without lockout or rate limiting
- JWT with `alg: none` accepted
- Tokens that never expire
- Credentials in URL parameters
- Missing password complexity requirements
**Severity**: Critical

### API3:2023 — Broken Object Property Level Authorization
**What**: User can read/write object properties they should not access.
**Test patterns**:
- Mass assignment: send `{"role":"admin"}` in update request
- Excessive data exposure: response contains password_hash, internal IDs, PII
- Check if read-only fields can be written via PUT/PATCH
**Severity**: High

### API4:2023 — Unrestricted Resource Consumption
**What**: No limits on request size, frequency, or returned data.
**Test patterns**:
- Request `?limit=999999` — does it return everything?
- Upload extremely large file — is there a size limit?
- Send 100 requests/second — is there rate limiting?
- Request deeply nested resources — does it cause server strain?
**Severity**: Medium to High

### API5:2023 — Broken Function Level Authorization
**What**: Regular users can access admin-only endpoints.
**Test patterns**:
- Access `/admin/*` endpoints with regular user token
- Change HTTP method (GET to DELETE) to bypass authorization
- Access internal/management endpoints from external network
**Severity**: Critical

### API6:2023 — Unrestricted Access to Sensitive Business Flows
**What**: Business logic can be abused at scale (ticket scalping, credential stuffing).
**Test patterns**:
- Rapid repeated purchase/redeem/signup requests
- Same coupon applied multiple times
- Account creation flood without CAPTCHA
**Severity**: Medium to High

### API7:2023 — Server Side Request Forgery (SSRF)
**What**: API can be tricked into making requests to internal resources.
**Test patterns**:
- URL parameters pointing to `http://169.254.169.254/` (cloud metadata)
- URL parameters pointing to `http://localhost:PORT/` (internal services)
- URL parameters with `file:///etc/passwd` (local file read)
**Severity**: High to Critical

### API8:2023 — Security Misconfiguration
**What**: Missing security headers, verbose errors, default credentials.
**Test patterns**:
- Check for security headers (X-Frame-Options, CSP, HSTS, X-Content-Type-Options)
- Check CORS policy (Access-Control-Allow-Origin: * is too permissive)
- Check for stack traces in error responses
- Check for debug/actuator endpoints exposed
- Check TLS configuration (version, cipher suites)
**Severity**: Medium

### API9:2023 — Improper Inventory Management
**What**: Old API versions, undocumented endpoints, shadow APIs.
**Test patterns**:
- Probe `/v1/`, `/v2/`, `/api/v1/` for old versions
- Check for internal endpoints (`/internal/`, `/debug/`, `/metrics/`)
- Compare documented endpoints vs actually available endpoints
- Check for GraphQL introspection enabled
**Severity**: Medium

### API10:2023 — Unsafe Consumption of APIs
**What**: API blindly trusts data from third-party APIs or user input without validation.
**Test patterns**:
- SQL injection in query parameters and JSON fields
- NoSQL injection (`{"$gt": ""}` in MongoDB queries)
- XSS payloads in stored fields
- Command injection in parameters used in server-side commands
- Path traversal (`../../etc/passwd`) in file parameters
**Severity**: High to Critical

---

## Performance Testing with wrk

### Basic Usage
```bash
# 2 threads, 10 connections, 30 seconds
wrk -t2 -c10 -d30s http://api.example.com/endpoint

# With custom headers
wrk -t2 -c10 -d30s -H "Authorization: Bearer TOKEN" http://api.example.com/endpoint

# With Lua script for POST requests
wrk -t2 -c10 -d30s -s post.lua http://api.example.com/endpoint
```

### Lua Script for POST (post.lua)
```lua
wrk.method = "POST"
wrk.headers["Content-Type"] = "application/json"
wrk.body = '{"key": "value"}'
```

### Interpreting wrk Output
```
Running 30s test @ http://api.example.com/users
  2 threads and 10 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency    12.34ms   5.67ms  89.12ms   78.90%
    Req/Sec   405.12    45.67   523.00     72.34%
  24000 requests in 30.01s, 12.34MB read
Requests/sec:   799.87
Transfer/sec:    421.12KB
```
- **Latency Avg**: Average response time per request
- **Latency Max**: Worst-case response time (tail latency)
- **Req/Sec**: Throughput per thread
- **Requests/sec (bottom)**: Total throughput
- **+/- Stdev**: Consistency (higher = more consistent)

### Performance Benchmarks
| Endpoint Type | Good | Acceptable | Slow | Critical |
|--------------|------|------------|------|----------|
| Health check | <10ms | <50ms | <200ms | >200ms |
| Simple GET (by ID) | <50ms | <200ms | <500ms | >500ms |
| List with pagination | <100ms | <500ms | <1000ms | >1000ms |
| Search/filter | <200ms | <500ms | <2000ms | >2000ms |
| Create (POST) | <100ms | <500ms | <1000ms | >1000ms |
| File upload | <500ms | <2000ms | <5000ms | >5000ms |

---

## Response Schema Validation Patterns

### JSON Schema Basics
```python
# Validate a response against an expected schema
def validate_response(actual, expected_type, required_fields=None, field_types=None):
    errors = []

    if expected_type == "object" and not isinstance(actual, dict):
        errors.append(f"Expected object, got {type(actual).__name__}")
        return errors

    if expected_type == "array" and not isinstance(actual, list):
        errors.append(f"Expected array, got {type(actual).__name__}")
        return errors

    if required_fields and isinstance(actual, dict):
        for field in required_fields:
            if field not in actual:
                errors.append(f"Missing required field: {field}")

    if field_types and isinstance(actual, dict):
        for field, expected in field_types.items():
            if field in actual and not isinstance(actual[field], expected):
                errors.append(f"Field '{field}' expected {expected.__name__}, got {type(actual[field]).__name__}")

    return errors
```

### Common Response Patterns to Validate
```
Single resource:  {"id": "...", "type": "...", "attributes": {...}}
Collection:       [{"id": "..."}, ...] or {"data": [...], "meta": {"total": N}}
Error:            {"error": {"code": "...", "message": "..."}}
Paginated:        {"data": [...], "page": 1, "per_page": 20, "total": 100}
```

---

## Common API Vulnerabilities and Detection

### Information Disclosure
```bash
# Stack traces in errors
curl -s "$BASE_URL/api/nonexistent" | grep -iE "stack|trace|exception|error.*at.*line"

# Server version in headers
curl -sI "$BASE_URL/" | grep -iE "^server:|^x-powered-by:"

# Internal IPs in responses
curl -s "$BASE_URL/api/health" | grep -oE "10\.[0-9]+\.[0-9]+\.[0-9]+|172\.(1[6-9]|2[0-9]|3[01])\.[0-9]+\.[0-9]+|192\.168\.[0-9]+\.[0-9]+"
```

### Injection Testing Quick Reference
| Type | Payload | Where to Test |
|------|---------|---------------|
| SQL (string) | `' OR '1'='1` | Query params, JSON string fields |
| SQL (numeric) | `1 OR 1=1` | Numeric query params, IDs |
| SQL (time-based) | `'; WAITFOR DELAY '0:0:5'--` | Any input (detect via timing) |
| NoSQL | `{"$gt": ""}` | JSON fields queried by MongoDB |
| XSS (reflected) | `<script>alert(1)</script>` | Query params reflected in response |
| XSS (stored) | `<img src=x onerror=alert(1)>` | POST body fields rendered in UI |
| Command | `; ls -la` | Params used in server shell commands |
| Path traversal | `../../etc/passwd` | File path parameters |
| SSRF | `http://169.254.169.254/` | URL parameters |

---

## Test Report Template Structure

### Executive Summary
- Total endpoints tested
- Pass/fail counts and percentages
- Critical findings count
- Overall risk assessment (Low/Medium/High/Critical)

### Detailed Results
For each endpoint:
- HTTP method and path
- Tests executed with expected vs actual results
- Response time measurements
- Any findings with severity

### Security Findings
For each finding:
- Unique ID (S-001, S-002, ...)
- OWASP category mapping
- Severity (Critical/High/Medium/Low/Info)
- Affected endpoint(s)
- Description of the vulnerability
- Reproduction steps (exact curl command)
- Recommended fix
- Evidence (response snippet or screenshot)

### Performance Summary
- Response time distribution per endpoint
- Endpoints exceeding target threshold
- Throughput under load (if load tested)
- Bottleneck identification

### Recommendations
Prioritized list of actions:
1. Critical: Fix immediately (auth bypass, injection, data exposure)
2. High: Fix within sprint (broken authorization, SSRF)
3. Medium: Fix within month (missing headers, weak rate limits)
4. Low: Fix when convenient (information disclosure, old API versions)
5. Info: Best practice suggestions

---

## REST API Best Practices Checklist

### Authentication & Authorization
- [ ] All endpoints require authentication (except public ones)
- [ ] Tokens have reasonable expiry
- [ ] Failed auth returns 401 (not 200 with error body)
- [ ] Authorization checked at object level (not just endpoint level)
- [ ] Rate limiting on auth endpoints

### Input Validation
- [ ] All inputs validated (type, length, range, format)
- [ ] Invalid input returns 400 with descriptive error
- [ ] No SQL/NoSQL/command injection possible
- [ ] File uploads validated (type, size, content)
- [ ] Request body size limited

### Response Quality
- [ ] Consistent response format across all endpoints
- [ ] Proper HTTP status codes used
- [ ] Error responses include actionable messages
- [ ] No sensitive data in responses (passwords, internal IDs, stack traces)
- [ ] Pagination implemented for list endpoints

### Security Headers
- [ ] `Strict-Transport-Security` (HSTS)
- [ ] `X-Content-Type-Options: nosniff`
- [ ] `X-Frame-Options: DENY`
- [ ] `Content-Security-Policy` set appropriately
- [ ] CORS configured for specific origins (not wildcard)
- [ ] `Cache-Control` set appropriately for sensitive data

### Performance
- [ ] Response times within target for all endpoints
- [ ] Pagination with default and maximum page sizes
- [ ] Compression enabled (gzip/br)
- [ ] Caching headers set where appropriate
- [ ] No N+1 query patterns detectable via timing

### Documentation
- [ ] OpenAPI/Swagger spec available and accurate
- [ ] All endpoints documented with examples
- [ ] Error codes and messages documented
- [ ] Rate limits documented
- [ ] Authentication flow documented
