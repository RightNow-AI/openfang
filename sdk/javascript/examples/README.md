# JavaScript Examples

These examples demonstrate the application-facing contract described in [docs/integration-contract.md](../../docs/integration-contract.md).

## Examples

### basic.js

Creates an agent directly and sends one message through the SDK.

### streaming.js

Streams agent output directly from OpenFang through the SDK.

### backend-proxy-server.js

Demonstrates the recommended production shape:

```text
Frontend -> your backend -> OpenFang
```

It exposes:

- `POST /api/ai/chat`
- `POST /api/ai/chat/stream`
- `GET /api/health`

The example keeps `userId -> agentId` in memory for simplicity. Replace that with your database in real applications.

### sse-client.js

Consumes the backend proxy's SSE endpoint and prints streamed deltas.

### nextjs-app-router/

Shows the same backend-owned pattern implemented as Next.js App Router route handlers.
See `nextjs-app-router/README.md` for the route files and setup notes.

## Environment

```bash
OPENFANG_BASE_URL=http://127.0.0.1:50051
OPENFANG_API_KEY=
OPENFANG_DEFAULT_TEMPLATE=assistant
APP_BACKEND_PORT=3100
APP_BACKEND_BASE_URL=http://127.0.0.1:3100
```

## Run the proxy example

```bash
cd sdk/javascript/examples
node backend-proxy-server.js
```

Then in a second terminal:

```bash
cd sdk/javascript/examples
node sse-client.js
```

Or send a non-streaming request:

```bash
curl -X POST http://127.0.0.1:3100/api/ai/chat \
  -H "Content-Type: application/json" \
  -d '{"userId":"demo-user","message":"What can you help me with?"}'
```
