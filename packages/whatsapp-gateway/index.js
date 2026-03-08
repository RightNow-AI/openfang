#!/usr/bin/env node
'use strict';

const http = require('node:http');
const { randomUUID } = require('node:crypto');

// ---------------------------------------------------------------------------
// Config from environment
// ---------------------------------------------------------------------------
const PORT = parseInt(process.env.WHATSAPP_GATEWAY_PORT || '3009', 10);
const OPENFANG_URL = (() => {
  const DEFAULT_URL = 'http://127.0.0.1:4200';
  const SAFE_HOSTS = new Set(['localhost', '127.0.0.1', '::1', '0.0.0.0']);
  const raw = (process.env.OPENFANG_URL || DEFAULT_URL).replace(/\/+$/, '');
  try {
    const parsed = new URL(raw);
    if (!SAFE_HOSTS.has(parsed.hostname)) {
      console.warn(
        `[gateway] OPENFANG_URL hostname "${parsed.hostname}" is not a safe loopback address. ` +
        `Falling back to ${DEFAULT_URL}`
      );
      return DEFAULT_URL;
    }
    return raw;
  } catch {
    console.warn(`[gateway] OPENFANG_URL "${raw}" is not a valid URL. Falling back to ${DEFAULT_URL}`);
    return DEFAULT_URL;
  }
})();
const DEFAULT_AGENT = process.env.OPENFANG_DEFAULT_AGENT || 'assistant';
const ALLOWED_NUMBERS = (process.env.WHATSAPP_ALLOWED_USERS || '')
  .split(',')
  .map(s => s.trim())
  .filter(Boolean);
const MAX_MESSAGE_LENGTH = 4096;

// Heartbeat watchdog — detects stale connections after system sleep/wake
const HEARTBEAT_INTERVAL_MS = 30_000;  // Check every 30 seconds
const HEARTBEAT_STALE_MS = 90_000;     // Consider stale if no Baileys activity for 90s

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------
let sock = null;          // Baileys socket
let sessionId = '';       // current session identifier
let qrDataUrl = '';       // latest QR code as data:image/png;base64,...
let connStatus = 'disconnected'; // disconnected | qr_ready | connected
let qrExpired = false;
let statusMessage = 'Not started';
let lastActivityAt = 0;       // timestamp of last known good Baileys activity
let heartbeatTimer = null;    // setInterval handle for heartbeat watchdog
let reconnecting = false;     // guard against overlapping reconnect attempts
let conflictCount = 0;        // consecutive conflict disconnects (for backoff)
const startedAt = Date.now(); // process start time

// ---------------------------------------------------------------------------
// Heartbeat watchdog — self-heals dead WebSocket after sleep/wake
// ---------------------------------------------------------------------------
function touchActivity() {
  lastActivityAt = Date.now();
}

function startHeartbeat() {
  stopHeartbeat();
  lastActivityAt = Date.now();

  heartbeatTimer = setInterval(async () => {
    if (connStatus !== 'connected' || reconnecting) return;

    const silentMs = Date.now() - lastActivityAt;
    if (silentMs > HEARTBEAT_STALE_MS) {
      console.log(`[gateway] Heartbeat: no activity for ${Math.round(silentMs / 1000)}s, probing...`);
      try {
        // Check if Baileys WebSocket is truly alive
        const wsOk = sock && sock.ws && sock.ws.readyState === 1; // WebSocket.OPEN
        const userOk = sock && sock.user;
        if (!wsOk || !userOk) {
          console.log(`[gateway] Heartbeat: dead socket (ws=${wsOk}, user=${userOk}), reconnecting`);
          await triggerReconnect();
          return;
        }
        // Socket looks alive — reset timer
        touchActivity();
      } catch (err) {
        console.log(`[gateway] Heartbeat probe error: ${err.message}, reconnecting`);
        await triggerReconnect();
      }
    }
  }, HEARTBEAT_INTERVAL_MS);
}

function stopHeartbeat() {
  if (heartbeatTimer) {
    clearInterval(heartbeatTimer);
    heartbeatTimer = null;
  }
}

async function triggerReconnect() {
  if (reconnecting) return;
  reconnecting = true;

  console.log('[gateway] Self-healing: initiating reconnect...');
  connStatus = 'reconnecting';
  statusMessage = 'Reconnecting (auto-heal)...';

  // Clean up existing socket
  if (sock) {
    try { sock.end(); } catch {}
    sock = null;
  }
  stopHeartbeat();

  // Brief delay then reconnect
  await new Promise(r => setTimeout(r, 3000));
  try {
    await startConnection();
  } catch (err) {
    console.error('[gateway] Self-heal reconnect failed:', err.message);
    // Retry after backoff
    setTimeout(() => {
      reconnecting = false;
      triggerReconnect();
    }, 10_000);
    return;
  }
  reconnecting = false;
}

// ---------------------------------------------------------------------------
// Crypto error tracking — detect stale encryption after macOS sleep
// ---------------------------------------------------------------------------
let cryptoErrorCount = 0;
const CRYPTO_ERROR_LIMIT = 3; // Clear auth_store after this many consecutive crypto errors

// ---------------------------------------------------------------------------
// Baileys connection
// ---------------------------------------------------------------------------
async function startConnection() {
  // Dynamic imports — Baileys is ESM-only in v6+
  const { default: makeWASocket, useMultiFileAuthState, DisconnectReason, fetchLatestBaileysVersion } =
    await import('@whiskeysockets/baileys');
  const QRCode = (await import('qrcode')).default || await import('qrcode');
  const pino = (await import('pino')).default || await import('pino');

  const logger = pino({ level: 'warn' });
  const authDir = require('node:path').join(__dirname, 'auth_store');

  const { state, saveCreds } = await useMultiFileAuthState(
    require('node:path').join(__dirname, 'auth_store')
  );
  const { version } = await fetchLatestBaileysVersion();

  sessionId = randomUUID();
  qrDataUrl = '';
  qrExpired = false;
  connStatus = 'disconnected';
  statusMessage = 'Connecting...';

  // In-memory message store for retry handling
  const msgStore = {};
  const MSG_STORE_MAX = 500;
  const msgStoreKeys = [];

  sock = makeWASocket({
    version,
    auth: state,
    logger,
    printQRInTerminal: true,
    browser: ['OpenFang', 'Desktop', '1.0.0'],
    // Required for Baileys 6.x to handle pre-key message retries
    getMessage: async (key) => {
      const id = key.remoteJid + ':' + key.id;
      return msgStore[id] || undefined;
    },
  });

  // Save credentials whenever they update
  sock.ev.on('creds.update', () => {
    touchActivity();
    saveCreds();
  });

  // Connection state changes (QR code, connected, disconnected)
  sock.ev.on('connection.update', async (update) => {
    touchActivity();
    const { connection, lastDisconnect, qr } = update;

    if (qr) {
      // New QR code generated — convert to data URL
      try {
        qrDataUrl = await QRCode.toDataURL(qr, { width: 256, margin: 2 });
        connStatus = 'qr_ready';
        qrExpired = false;
        statusMessage = 'Scan this QR code with WhatsApp → Linked Devices';
        console.log('[gateway] QR code ready — waiting for scan');
      } catch (err) {
        console.error('[gateway] QR generation failed:', err.message);
      }
    }

    if (connection === 'close') {
      stopHeartbeat();
      const statusCode = lastDisconnect?.error?.output?.statusCode;
      const reason = lastDisconnect?.error?.output?.payload?.message || 'unknown';
      console.log(`[gateway] Connection closed: ${reason} (${statusCode})`);

      if (statusCode === DisconnectReason.loggedOut) {
        // User logged out from phone — clear auth and stop
        connStatus = 'disconnected';
        statusMessage = 'Logged out. Generate a new QR code to reconnect.';
        qrDataUrl = '';
        sock = null;
        // Remove auth store so next connect gets a fresh QR
        const fs = require('node:fs');
        const path = require('node:path');
        const authPath = path.join(__dirname, 'auth_store');
        if (fs.existsSync(authPath)) {
          fs.rmSync(authPath, { recursive: true, force: true });
        }
      } else if (statusCode === 440 || reason.includes('conflict')) {
        // Conflict — another session replaced us. Back off to avoid ping-pong loop.
        conflictCount += 1;
        const backoff = Math.min(conflictCount * 15_000, 60_000); // 15s, 30s, 45s, max 60s
        console.log(`[gateway] Conflict disconnect #${conflictCount}, backing off ${backoff / 1000}s`);
        connStatus = 'reconnecting';
        statusMessage = `Conflict — retrying in ${backoff / 1000}s`;
        setTimeout(() => startConnection(), backoff);
      } else {
        // Check for crypto/encryption errors (stale sessions after macOS sleep)
        const fullError = lastDisconnect?.error?.message || reason;
        const isCryptoError = /Bad MAC|decrypt|No matching sessions|getAvailablePreKeysOnServer/i.test(fullError);

        if (isCryptoError) {
          cryptoErrorCount += 1;
          console.warn(`[gateway] Crypto error #${cryptoErrorCount}/${CRYPTO_ERROR_LIMIT}: ${fullError}`);

          if (cryptoErrorCount >= CRYPTO_ERROR_LIMIT) {
            console.warn('[gateway] Repeated crypto errors — clearing auth_store for fresh session');
            cryptoErrorCount = 0;
            const fs = require('node:fs');
            const path = require('node:path');
            const authPath = path.join(__dirname, 'auth_store');
            if (fs.existsSync(authPath)) {
              fs.rmSync(authPath, { recursive: true, force: true });
            }
            connStatus = 'disconnected';
            qrDataUrl = '';
            statusMessage = 'Auth expired — scan QR code again';
            console.log('[gateway] Auth cleared. Reconnecting for fresh QR...');
            setTimeout(() => startConnection(), 5000);
            return;
          }
        } else {
          cryptoErrorCount = 0;
        }

        // All other disconnects (restart required, timeout, unknown) — auto-reconnect
        conflictCount = 0;
        connStatus = 'reconnecting';
        console.log('[gateway] Reconnecting in 3s...');
        statusMessage = 'Reconnecting...';
        setTimeout(() => startConnection(), 3000);
      }
    }

    if (connection === 'open') {
      connStatus = 'connected';
      qrExpired = false;
      qrDataUrl = '';
      statusMessage = 'Connected to WhatsApp';
      reconnecting = false;
      conflictCount = 0;
      cryptoErrorCount = 0;
      console.log('[gateway] Connected to WhatsApp!');
      startHeartbeat();
    }
  });

  // Incoming messages → forward to OpenFang
  sock.ev.on('messages.upsert', async ({ messages, type }) => {
    touchActivity();
    if (type !== 'notify') return;

    for (const msg of messages) {
      // Store message for retry handling (with LRU eviction)
      if (msg.key.id && msg.key.remoteJid) {
        const storeKey = msg.key.remoteJid + ':' + msg.key.id;
        msgStore[storeKey] = msg.message;
        msgStoreKeys.push(storeKey);
        if (msgStoreKeys.length > MSG_STORE_MAX) {
          delete msgStore[msgStoreKeys.shift()];
        }
      }
      // Skip own outgoing messages (prevents echo loop)
      if (msg.key.fromMe) continue;
      // Skip status broadcasts
      if (msg.key.remoteJid === 'status@broadcast') continue;
      // Skip group messages — only process direct chats (JID ends with @s.whatsapp.net)
      if (msg.key.remoteJid && !msg.key.remoteJid.endsWith('@s.whatsapp.net')) continue;
      // Allowlist: only process messages from approved numbers (empty = allow all)
      if (ALLOWED_NUMBERS.length > 0) {
        const senderNum = (msg.key.remoteJid || '').replace(/@.*$/, '');
        if (!ALLOWED_NUMBERS.includes(senderNum)) {
          console.log(`[gateway] Blocked message from ${senderNum} (not in allowlist)`);
          continue;
        }
      }
      // Skip protocol/reaction/receipt messages (no useful text)
      if (msg.message?.protocolMessage || msg.message?.reactionMessage) continue;

      const sender = msg.key.remoteJid || '';
      let text = msg.message?.conversation
        || msg.message?.extendedTextMessage?.text
        || msg.message?.imageMessage?.caption
        || '';

      // Truncate oversized messages to prevent abuse
      if (text.length > MAX_MESSAGE_LENGTH) {
        console.log(`[gateway] Truncating message from ${text.length} to ${MAX_MESSAGE_LENGTH} chars`);
        text = text.substring(0, MAX_MESSAGE_LENGTH);
      }

      if (!text) continue;

      // Extract phone number from JID (e.g. "1234567890@s.whatsapp.net" → "+1234567890")
      const phone = '+' + sender.replace(/@.*$/, '');
      const pushName = msg.pushName || phone;

      console.log(`[gateway] Incoming from ${pushName} (${phone}): ${text.substring(0, 80)}`);

      // Forward to OpenFang agent
      try {
        const response = await forwardToOpenFang(text, phone, pushName);
        if (response && sock) {
          // Send agent response back to WhatsApp
          await sock.sendMessage(sender, { text: response });
          console.log(`[gateway] Replied to ${pushName}`);
        }
      } catch (err) {
        console.error(`[gateway] Forward/reply failed:`, err.message);
      }
    }
  });
}

// ---------------------------------------------------------------------------
// Resolve agent name to UUID (cached)
// ---------------------------------------------------------------------------
let resolvedAgentId = null;
let resolvedAgentAt = 0;
const AGENT_RESOLVE_TTL_MS = 5 * 60 * 1000; // 5 minutes

function resolveAgentId() {
  return new Promise((resolve, reject) => {
    if (resolvedAgentId && (Date.now() - resolvedAgentAt < AGENT_RESOLVE_TTL_MS)) return resolve(resolvedAgentId);
    const url = new URL(`${OPENFANG_URL}/api/agents`);
    const req = http.request(
      { hostname: url.hostname, port: url.port || 4200, path: url.pathname, method: 'GET', timeout: 10000 },
      (res) => {
        let body = '';
        res.on('data', (chunk) => { body += chunk; });
        res.on('end', () => {
          try {
            const agents = JSON.parse(body);
            const match = agents.find(a => a.name === DEFAULT_AGENT || a.id === DEFAULT_AGENT);
            if (match) {
              resolvedAgentId = match.id;
              resolvedAgentAt = Date.now();
              console.log(`[gateway] Resolved agent "${DEFAULT_AGENT}" → ${resolvedAgentId}`);
              resolve(resolvedAgentId);
            } else {
              // Fallback: use DEFAULT_AGENT as-is (might be a UUID already)
              resolve(DEFAULT_AGENT);
            }
          } catch (e) { resolve(DEFAULT_AGENT); }
        });
      }
    );
    req.on('error', () => resolve(DEFAULT_AGENT));
    req.end();
  });
}

// ---------------------------------------------------------------------------
// Forward incoming message to OpenFang API, return agent response
// ---------------------------------------------------------------------------
function forwardToOpenFang(text, phone, pushName) {
  return resolveAgentId().then((agentId) => new Promise((resolve, reject) => {
    const payload = JSON.stringify({
      message: `[WhatsApp from ${pushName} (${phone})]: ${text}`,
    });

    const url = new URL(`${OPENFANG_URL}/api/agents/${encodeURIComponent(agentId)}/message`);

    const req = http.request(
      {
        hostname: url.hostname,
        port: url.port || 4200,
        path: url.pathname,
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Content-Length': Buffer.byteLength(payload),
        },
        timeout: 600_000, // Video processing pipelines can take several minutes
      },
      (res) => {
        let body = '';
        res.on('data', (chunk) => (body += chunk));
        res.on('end', () => {
          try {
            const data = JSON.parse(body);
            // The /api/agents/{id}/message endpoint returns { response: "..." }
            resolve(data.response || data.message || data.text || '');
          } catch {
            resolve(body.trim() || '');
          }
        });
      },
    );

    req.on('error', reject);
    req.on('timeout', () => {
      req.destroy();
      reject(new Error('OpenFang API timeout'));
    });
    req.write(payload);
    req.end();
  }));
}

// ---------------------------------------------------------------------------
// Send a message via Baileys (called by OpenFang for outgoing)
// ---------------------------------------------------------------------------
async function sendMessage(to, text) {
  if (!sock || connStatus !== 'connected') {
    throw new Error('WhatsApp not connected');
  }

  // Normalize phone → JID: "+1234567890" → "1234567890@s.whatsapp.net"
  const jid = to.replace(/^\+/, '').replace(/@.*$/, '') + '@s.whatsapp.net';

  await sock.sendMessage(jid, { text });
}

// ---------------------------------------------------------------------------
// HTTP server
// ---------------------------------------------------------------------------
function parseBody(req) {
  return new Promise((resolve, reject) => {
    let body = '';
    req.on('data', (chunk) => (body += chunk));
    req.on('end', () => {
      try {
        resolve(body ? JSON.parse(body) : {});
      } catch (e) {
        reject(new Error('Invalid JSON'));
      }
    });
    req.on('error', reject);
  });
}

function jsonResponse(res, status, data) {
  const body = JSON.stringify(data);
  res.writeHead(status, {
    'Content-Type': 'application/json',
    'Content-Length': Buffer.byteLength(body),
    'Access-Control-Allow-Origin': '*',
  });
  res.end(body);
}

const server = http.createServer(async (req, res) => {
  // CORS preflight
  if (req.method === 'OPTIONS') {
    res.writeHead(204, {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type',
    });
    return res.end();
  }

  const url = new URL(req.url, `http://localhost:${PORT}`);
  const path = url.pathname;

  try {
    // POST /login/start — start Baileys connection, return QR
    if (req.method === 'POST' && path === '/login/start') {
      // If already connected, just return success
      if (connStatus === 'connected') {
        return jsonResponse(res, 200, {
          qr_data_url: '',
          session_id: sessionId,
          message: 'Already connected to WhatsApp',
          connected: true,
        });
      }

      // Clean up existing socket to prevent leaked event listeners
      if (sock) {
        try { sock.end(); } catch {}
        sock = null;
      }

      // Start a new connection (resets any existing)
      await startConnection();

      // Wait briefly for QR to generate (Baileys emits it quickly)
      let waited = 0;
      while (!qrDataUrl && connStatus !== 'connected' && waited < 15_000) {
        await new Promise((r) => setTimeout(r, 300));
        waited += 300;
      }

      return jsonResponse(res, 200, {
        qr_data_url: qrDataUrl,
        session_id: sessionId,
        message: statusMessage,
        connected: connStatus === 'connected',
      });
    }

    // GET /login/status — poll for connection status
    if (req.method === 'GET' && path === '/login/status') {
      return jsonResponse(res, 200, {
        connected: connStatus === 'connected',
        message: statusMessage,
        expired: qrExpired,
      });
    }

    // POST /message/send — send outgoing message via Baileys
    if (req.method === 'POST' && path === '/message/send') {
      const body = await parseBody(req);
      const { to, text } = body;

      if (!to || !text) {
        return jsonResponse(res, 400, { error: 'Missing "to" or "text" field' });
      }

      await sendMessage(to, text);
      return jsonResponse(res, 200, { success: true, message: 'Sent' });
    }

    // GET /health — health check (enhanced with diagnostics)
    if (req.method === 'GET' && path === '/health') {
      return jsonResponse(res, 200, {
        status: 'ok',
        connected: connStatus === 'connected',
        conn_status: connStatus,
        session_id: sessionId || null,
        has_socket: sock !== null,
        last_activity_ms: lastActivityAt ? (Date.now() - lastActivityAt) : null,
        uptime_ms: Date.now() - startedAt,
      });
    }

    // POST /health/reconnect — kernel-triggered reconnect
    if (req.method === 'POST' && path === '/health/reconnect') {
      if (connStatus === 'connected' && sock) {
        return jsonResponse(res, 200, {
          reconnected: false,
          reason: 'already_connected',
        });
      }
      if (connStatus === 'reconnecting' || reconnecting) {
        return jsonResponse(res, 200, {
          reconnected: false,
          reason: 'already_reconnecting',
        });
      }
      triggerReconnect();
      return jsonResponse(res, 200, {
        reconnected: true,
        message: 'Reconnect initiated',
      });
    }

    // 404
    jsonResponse(res, 404, { error: 'Not found' });
  } catch (err) {
    console.error(`[gateway] ${req.method} ${path} error:`, err.message);
    jsonResponse(res, 500, { error: err.message });
  }
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`[gateway] WhatsApp Web gateway listening on http://127.0.0.1:${PORT}`);
  console.log(`[gateway] OpenFang URL: ${OPENFANG_URL}`);
  console.log(`[gateway] Default agent: ${DEFAULT_AGENT}`);

  // Auto-connect if auth_store exists (previous session saved)
  const authDir = require('node:path').join(__dirname, 'auth_store');
  const fs = require('node:fs');
  if (fs.existsSync(authDir) && fs.readdirSync(authDir).length > 0) {
    console.log('[gateway] Found existing auth session — auto-connecting...');
    startConnection().catch(err => {
      console.error('[gateway] Auto-connect failed:', err.message);
      console.log('[gateway] Waiting for POST /login/start to begin QR flow...');
    });
  } else {
    console.log('[gateway] No saved session. Waiting for POST /login/start to begin QR flow...');
  }
});

// Graceful shutdown
process.on('SIGINT', () => {
  console.log('\n[gateway] Shutting down...');
  stopHeartbeat();
  if (sock) sock.end();
  server.close(() => process.exit(0));
});

process.on('SIGTERM', () => {
  stopHeartbeat();
  if (sock) sock.end();
  server.close(() => process.exit(0));
});
