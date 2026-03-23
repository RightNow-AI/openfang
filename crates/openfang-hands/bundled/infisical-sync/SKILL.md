---
name: infisical-sync-skill
version: "1.0.0"
description: "Expert knowledge for the Infisical Sync Hand — Infisical API reference, vault operations, error patterns, security guidance"
author: OpenFang
tags: [secrets, infisical, vault, security, sync]
tools: [shell_exec, vault_set, vault_get, vault_list, vault_delete, memory_store, memory_recall]
runtime: prompt_only
---

# Infisical Sync Expert Knowledge

## 1. Infisical API Reference

### Base URL
All requests go to `$INFISICAL_URL`. This is the self-hosted instance base URL, e.g. `https://infisical.example.com`.

### Authentication — Universal Auth
Infisical uses Machine Identities with Universal Auth for agent-to-agent communication.

**Endpoint**: `POST /api/v1/auth/universal-auth/login`

**Request**:
```json
{
  "clientId": "<INFISICAL_CLIENT_ID>",
  "clientSecret": "<INFISICAL_CLIENT_SECRET>"
}
```

**Response** (success):
```json
{
  "accessToken": "eyJ...",
  "expiresIn": 7200,
  "accessTokenMaxTTL": 43200,
  "tokenType": "Bearer"
}
```

**curl example**:
```bash
RESPONSE=$(curl -s -X POST "$INFISICAL_URL/api/v1/auth/universal-auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"clientId\":\"$INFISICAL_CLIENT_ID\",\"clientSecret\":\"$INFISICAL_CLIENT_SECRET\"}")

ACCESS_TOKEN=$(echo "$RESPONSE" | python3 -c "import sys,json; print(json.load(sys.stdin)['accessToken'])")
```

Token lifetime: `expiresIn` seconds (usually 7200 = 2 hours). Re-authenticate when expired.

---

### List Secrets
**Endpoint**: `GET /api/v4/secrets`

**Query parameters**:
| Param | Required | Description |
|-------|----------|-------------|
| `projectId` | Yes | Infisical project ID |
| `environment` | Yes | Environment slug (e.g. `prod`, `staging`, `dev`) |
| `secretPath` | No | Path prefix, default `/` |
| `includeImports` | No | Include imported secrets, default `false` |
| `recursive` | No | Include secrets in sub-paths, default `false` |

**curl example**:
```bash
curl -s -X GET \
  "$INFISICAL_URL/api/v4/secrets?projectId=$PROJECT_ID&environment=$ENVIRONMENT&secretPath=/" \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

**Response shape**:
```json
{
  "secrets": [
    {
      "id": "uuid",
      "version": 1,
      "secretKey": "DATABASE_URL",
      "secretValue": "postgres://...",
      "secretComment": "",
      "environment": "prod",
      "workspace": "uuid"
    }
  ],
  "imports": []
}
```

Parse with:
```bash
echo "$RESPONSE" | python3 -c "
import sys, json
data = json.load(sys.stdin)
for s in data.get('secrets', []):
    print(s['secretKey'])
"
```

---

### Create or Update a Secret

The API does **not** provide a single upsert endpoint. `POST` creates only (returns 409 if the secret already exists); `PATCH` updates only (returns 404 if missing). Use the create-then-update pattern:

**Step 1 — Try to create (POST)**  
**Endpoint**: `POST /api/v4/secrets/{secretName}`

**Request body**:
```json
{
  "projectId": "<PROJECT_ID>",
  "environment": "<ENV>",
  "secretValue": "<VALUE>",
  "secretPath": "/"
}
```

```bash
HTTP_STATUS=$(curl -s -o /tmp/infisical_response.json -w "%{http_code}" \
  -X POST "$INFISICAL_URL/api/v4/secrets/$SECRET_NAME" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"projectId\":\"$PROJECT_ID\",\"environment\":\"$ENVIRONMENT\",\"secretValue\":\"$SECRET_VALUE\",\"secretPath\":\"/\"}")
```

Returns `201` on success.

**Step 2 — If 409, update via PATCH**  
**Endpoint**: `PATCH /api/v4/secrets/{secretName}`

```bash
if [ "$HTTP_STATUS" = "409" ]; then
  HTTP_STATUS=$(curl -s -o /tmp/infisical_response.json -w "%{http_code}" \
    -X PATCH "$INFISICAL_URL/api/v4/secrets/$SECRET_NAME" \
    -H "Authorization: Bearer $ACCESS_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{\"projectId\":\"$PROJECT_ID\",\"environment\":\"$ENVIRONMENT\",\"secretValue\":\"$SECRET_VALUE\",\"secretPath\":\"/\"}")
fi
```

Returns `200` on success. Any other status code is an error.

**Important**: URL-encode the secret name if it contains special characters.

---

### Delete a Secret
**Endpoint**: `DELETE /api/v4/secrets/{secretName}`

**Query parameters**: `projectId`, `environment`, `secretPath` (default `/`)

**curl example**:
```bash
curl -s -X DELETE \
  "$INFISICAL_URL/api/v4/secrets/$SECRET_NAME?projectId=$PROJECT_ID&environment=$ENVIRONMENT&secretPath=/" \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

---

### List Accessible Projects (Workspaces)
**Endpoint**: `GET /api/v1/workspace`

```bash
curl -s -X GET "$INFISICAL_URL/api/v1/workspace" \
  -H "Authorization: Bearer $ACCESS_TOKEN"
```

Response: `{ "workspaces": [{ "id": "uuid", "name": "...", "environments": [...] }] }`

---

## 2. HTTP Error Codes

| Code | Meaning | Action |
|------|---------|--------|
| 200/201 | Success | Continue |
| 400 | Bad Request | Log the response body — likely malformed JSON or missing field |
| 401 | Unauthorized | Re-authenticate; token may have expired |
| 403 | Forbidden | Machine identity lacks permissions — check Infisical Access Control |
| 404 | Not Found | Secret or project doesn't exist |
| 429 | Rate Limited | Wait 60 seconds, retry once |
| 500/503 | Server Error | Log + retry once after 30 seconds; notify if still failing |

Always check HTTP status before trusting response body:
```bash
HTTP_STATUS=$(curl -s -o /tmp/infisical_response.json -w "%{http_code}" ...)
if [ "$HTTP_STATUS" != "200" ] && [ "$HTTP_STATUS" != "201" ]; then
  # handle error
fi
RESPONSE=$(cat /tmp/infisical_response.json)
```

---

## 3. Sync State File Format

Stored at `infisical_sync_state.json`:
```json
{
  "last_sync": "2025-01-15T10:30:00Z",
  "project_ids": ["uuid1", "uuid2"],
  "environment": "prod",
  "secrets": {
    "DATABASE_URL": {
      "hash": "sha256_of_key_plus_value",
      "version": 3,
      "last_synced": "2025-01-15T10:30:00Z"
    }
  },
  "error_count": 0,
  "push_count": 12,
  "pull_count": 47
}
```

**Hash computation** (to detect changes without storing values):
```bash
echo -n "DATABASE_URL:postgres://..." | sha256sum | awk '{print $1}'
```

Or with Python:
```python
import hashlib
h = hashlib.sha256(f"{key}:{value}".encode()).hexdigest()
```

---

## 4. Vault Operations Reference

The local vault provides encrypted key-value storage. All secrets synced from Infisical go here.

| Operation | Description |
|-----------|-------------|
| `vault_set key=K value=V` | Write or overwrite secret K |
| `vault_get key=K` | Read secret K |
| `vault_list` | List all keys (values not returned) |
| `vault_delete key=K` | Delete secret K |

**Bulk sync pattern**:
```
// Pull from Infisical → vault
for each (key, value) in infisical_secrets:
    vault_set key=<key> value=<value>

// Optionally remove orphans
vault_list → local_keys
infisical_keys = set of keys returned by Infisical
for key in local_keys - infisical_keys:
    vault_delete key=<key>
```

---

## 5. Security Checklist

Before every sync cycle, verify:
- [ ] `INFISICAL_URL` is set and non-empty
- [ ] `INFISICAL_CLIENT_ID` is set and non-empty
- [ ] `INFISICAL_CLIENT_SECRET` is set and non-empty
- [ ] The access token was freshly obtained this cycle (never reuse across cycles)
- [ ] No secret values appear in curl command echo output (use variables, not inline values)
- [ ] Response body is never logged verbatim (strip `secretValue` fields before logging)

---

## 6. Common Failure Modes

### "Failed to fetch secrets: 403 Forbidden"
The Machine Identity exists but lacks permissions. In Infisical:
1. Go to Access Control → Machine Identities
2. Find this agent's identity
3. Assign it the `member` role (or `viewer` for read-only) on the project

### "Connection refused / Could not connect to server"
`INFISICAL_URL` is wrong or the instance is down. Verify the URL is reachable:
```bash
curl -s "$INFISICAL_URL/api/status" | python3 -c "import sys,json; print(json.load(sys.stdin))"
```

### "invalid character in secret name"
Secret names in Infisical must match `[A-Z0-9_]`. If the vault has mixed-case keys, normalise before pushing:
```bash
echo "my_secret_key" | tr '[:lower:]' '[:upper:]'
```

### "accessToken undefined in response"
Authentication failed. The response body will contain an error message. Check:
1. `INFISICAL_CLIENT_ID` and `INFISICAL_CLIENT_SECRET` are correct
2. The Machine Identity is not disabled in Infisical
3. The Machine Identity's token TTL hasn't been set to 0

---

## 7. Knowledge Graph Entities

Track fleet-wide secrets metadata without exposing values.

### Entity types
- `service` — the Infisical instance itself
- `secret_project` — an Infisical workspace/project
- `secret` — a named secret (key only, never value)

### Relation types
- `secret` → `belongs_to` → `secret_project`
- `secret_project` → `hosted_by` → `service`
- `secret` → `synced_to` → `agent_vault`

### Query examples
```
knowledge_query type=secret                  // list all known secrets
knowledge_query type=secret_project          // list all projects
knowledge_query relation=belongs_to target=<project_id>  // secrets in a project
```
