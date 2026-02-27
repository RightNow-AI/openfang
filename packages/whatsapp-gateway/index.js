#!/usr/bin/env node
'use strict';

const http = require('node:http');
const { randomUUID } = require('node:crypto');

// ---------------------------------------------------------------------------
// Config from environment
// ---------------------------------------------------------------------------
const PORT = parseInt(process.env.WHATSAPP_GATEWAY_PORT || '3009', 10);
const OPENFANG_URL = (process.env.OPENFANG_URL || 'http://127.0.0.1:4200').replace(/\/+$/, '');
const DEFAULT_AGENT = process.env.OPENFANG_DEFAULT_AGENT || 'assistant';

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------
let sock = null;          // Baileys socket
let sessionId = '';       // current session identifier
let qrDataUrl = '';       // latest QR code as data:image/png;base64,...
let connStatus = 'disconnected'; // disconnected | qr_ready | connected
let qrExpired = false;
let statusMessage = 'Not started';

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

  sock = makeWASocket({
    version,
    auth: state,
    logger,
    printQRInTerminal: true,
    browser: ['OpenFang', 'Desktop', '1.0.0'],
  });

  // Save credentials whenever they update
  sock.ev.on('creds.update', saveCreds);

  // Connection state changes (QR code, connected, disconnected)
  sock.ev.on('connection.update', async (update) => {
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
      } else if (statusCode === DisconnectReason.restartRequired ||
                 statusCode === DisconnectReason.timedOut) {
        // Recoverable — reconnect automatically
        console.log('[gateway] Reconnecting...');
        statusMessage = 'Reconnecting...';
        setTimeout(() => startConnection(), 2000);
      } else {
        // QR expired or other non-recoverable close
        qrExpired = true;
        connStatus = 'disconnected';
        statusMessage = 'QR code expired. Click "Generate New QR" to retry.';
        qrDataUrl = '';
      }
    }

    if (connection === 'open') {
      connStatus = 'connected';
      qrExpired = false;
      qrDataUrl = '';
      statusMessage = 'Connected to WhatsApp';
      console.log('[gateway] Connected to WhatsApp!');
    }
  });

  // Incoming messages → forward to OpenFang
  sock.ev.on('messages.upsert', async ({ messages, type }) => {
    if (type !== 'notify') return;

    for (const msg of messages) {
      // Skip own outgoing messages (prevents echo loop)
      if (msg.key.fromMe) continue;
      // Skip status broadcasts
      if (msg.key.remoteJid === 'status@broadcast') continue;
      // Skip group messages — only process direct chats (JID ends with @s.whatsapp.net)
      if (msg.key.remoteJid && !msg.key.remoteJid.endsWith('@s.whatsapp.net')) continue;
      // Skip protocol/reaction/receipt messages (no useful text)
      if (msg.message?.protocolMessage || msg.message?.reactionMessage) continue;

      const sender = msg.key.remoteJid || '';
      const text = msg.message?.conversation
        || msg.message?.extendedTextMessage?.text
        || msg.message?.imageMessage?.caption
        || '';

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

function resolveAgentId() {
  return new Promise((resolve, reject) => {
    if (resolvedAgentId) return resolve(resolvedAgentId);
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
      message: text,
      metadata: {
        channel: 'whatsapp',
        sender: phone,
        sender_name: pushName,
      },
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
        timeout: 120_000, // LLM calls can be slow
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

    // GET /health — health check
    if (req.method === 'GET' && path === '/health') {
      return jsonResponse(res, 200, {
        status: 'ok',
        connected: connStatus === 'connected',
        session_id: sessionId || null,
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
  console.log('[gateway] Waiting for POST /login/start to begin QR flow...');
});

// Graceful shutdown
process.on('SIGINT', () => {
  console.log('\n[gateway] Shutting down...');
  if (sock) sock.end();
  server.close(() => process.exit(0));
});

process.on('SIGTERM', () => {
  if (sock) sock.end();
  server.close(() => process.exit(0));
});
