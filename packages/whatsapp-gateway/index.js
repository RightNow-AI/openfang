#!/usr/bin/env node

import http from 'node:http';
import { randomUUID } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import makeWASocket, { useMultiFileAuthState, DisconnectReason, fetchLatestBaileysVersion, downloadMediaMessage } from '@whiskeysockets/baileys';
import QRCode from 'qrcode';
import pino from 'pino';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// ---------------------------------------------------------------------------
// Config from environment
// ---------------------------------------------------------------------------
const PORT = parseInt(process.env.WHATSAPP_GATEWAY_PORT || '3009', 10);
const OPENFANG_URL = (process.env.OPENFANG_URL || 'http://127.0.0.1:4200').replace(/\/+$/, '');
const DEFAULT_AGENT_NAME = process.env.OPENFANG_DEFAULT_AGENT || 'ambrogio';
let resolvedAgentId = null; // will be resolved on first use

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------
let sock = null;          // Baileys socket
let sessionId = '';       // current session identifier
let qrDataUrl = '';       // latest QR code as data:image/png;base64,...
let connStatus = 'disconnected'; // disconnected | qr_ready | connected
let qrExpired = false;
let statusMessage = 'Not started';
let reconnectAttempt = 0; // exponential backoff counter
const MAX_RECONNECT_DELAY = 60_000; // cap at 60s

// ---------------------------------------------------------------------------
// Baileys connection
// ---------------------------------------------------------------------------
async function startConnection() {
  const logger = pino({ level: 'warn' });
  const authDir = path.join(__dirname, 'auth_store');

  const { state, saveCreds } = await useMultiFileAuthState(authDir);
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
        // User logged out from phone — clear auth and stop (truly non-recoverable)
        connStatus = 'disconnected';
        statusMessage = 'Logged out. Generate a new QR code to reconnect.';
        qrDataUrl = '';
        sock = null;
        reconnectAttempt = 0;
        // Remove auth store so next connect gets a fresh QR
        const authPath = path.join(__dirname, 'auth_store');
        if (fs.existsSync(authPath)) {
          fs.rmSync(authPath, { recursive: true, force: true });
        }
      } else {
        // All other disconnect reasons are recoverable — reconnect with backoff
        // Covers: restartRequired(515), timedOut(408), connectionClosed(428),
        // connectionLost(408), connectionReplaced(440), badSession(500), etc.
        reconnectAttempt++;
        const delay = Math.min(1000 * Math.pow(2, reconnectAttempt - 1), MAX_RECONNECT_DELAY);
        console.log(`[gateway] Reconnecting in ${delay}ms (attempt ${reconnectAttempt})...`);
        statusMessage = `Reconnecting (attempt ${reconnectAttempt})...`;
        connStatus = 'disconnected';
        setTimeout(() => startConnection(), delay);
      }
    }

    if (connection === 'open') {
      connStatus = 'connected';
      qrExpired = false;
      qrDataUrl = '';
      reconnectAttempt = 0;
      statusMessage = 'Connected to WhatsApp';
      console.log('[gateway] Connected to WhatsApp!');
    }
  });

  // Incoming messages → forward to OpenFang
  sock.ev.on('messages.upsert', async ({ messages, type }) => {
    if (type !== 'notify') return;

    for (const msg of messages) {
      // Skip messages from self and status broadcasts
      if (msg.key.fromMe) continue;
      if (msg.key.remoteJid === 'status@broadcast') continue;

      const remoteJid = msg.key.remoteJid || '';
      const isGroup = remoteJid.endsWith('@g.us');

      // Detect media messages
      const mediaInfo = msg.message?.imageMessage
        ? { type: 'image', mime: msg.message.imageMessage.mimetype || 'image/jpeg', caption: msg.message.imageMessage.caption || '' }
        : msg.message?.videoMessage
        ? { type: 'video', mime: msg.message.videoMessage.mimetype || 'video/mp4', caption: msg.message.videoMessage.caption || '' }
        : msg.message?.audioMessage
        ? { type: 'audio', mime: msg.message.audioMessage.mimetype || 'audio/ogg', caption: '' }
        : msg.message?.documentMessage
        ? { type: 'document', mime: msg.message.documentMessage.mimetype || 'application/octet-stream', caption: msg.message.documentMessage.caption || '', filename: msg.message.documentMessage.fileName || 'document' }
        : msg.message?.stickerMessage
        ? { type: 'sticker', mime: msg.message.stickerMessage.mimetype || 'image/webp', caption: '' }
        : null;

      const text = msg.message?.conversation
        || msg.message?.extendedTextMessage?.text
        || mediaInfo?.caption
        || '';

      // Skip if no text AND no media
      if (!text && !mediaInfo) continue;

      // For groups: real sender is in participant; for DMs: it's remoteJid
      const senderJid = isGroup ? (msg.key.participant || '') : remoteJid;
      const phone = '+' + senderJid.replace(/@.*$/, '');
      const pushName = msg.pushName || phone;

      // Build metadata with group context
      const metadata = {
        channel: 'whatsapp',
        sender: phone,
        sender_name: pushName,
      };

      if (isGroup) {
        metadata.group_jid = remoteJid;
        metadata.group_name = msg.key.remoteJid; // basic group ID
        metadata.is_group = true;
      }

      // Download and upload media if present
      let attachments = [];
      if (mediaInfo) {
        try {
          const buffer = await downloadMediaMessage(msg, 'buffer', {});
          const ext = mediaInfo.mime.split('/').pop()?.split(';')[0] || 'bin';
          const filename = mediaInfo.filename || `${mediaInfo.type}.${ext}`;
          const agentId = await resolveAgentId(DEFAULT_AGENT_NAME);
          const fileId = await uploadToOpenFang(agentId, buffer, mediaInfo.mime, filename);
          attachments.push({ file_id: fileId, filename, content_type: mediaInfo.mime });
          console.log(`[gateway] Uploaded ${mediaInfo.type} (${(buffer.length / 1024).toFixed(1)}KB) → ${fileId}`);
        } catch (err) {
          console.error(`[gateway] Media download/upload failed:`, err.message);
          // Still forward the text/caption if available
        }
      }

      const logText = text ? text.substring(0, 80) : `[${mediaInfo?.type || 'media'}]`;
      if (isGroup) {
        console.log(`[gateway] Group msg from ${pushName} (${phone}) in ${remoteJid}: ${logText}`);
      } else {
        console.log(`[gateway] Incoming from ${pushName} (${phone}): ${logText}`);
      }

      // Forward to OpenFang agent
      const messageText = text || `[${mediaInfo?.type || 'media'} received]`;
      try {
        const response = await forwardToOpenFang(messageText, phone, pushName, metadata, attachments);
        if (response && sock) {
          // Reply in the same context: group → group, DM → DM
          const replyJid = isGroup ? remoteJid : senderJid.replace(/@.*$/, '') + '@s.whatsapp.net';
          await sock.sendMessage(replyJid, { text: response });
          console.log(`[gateway] Replied to ${pushName}${isGroup ? ' in group ' + remoteJid : ' privately'}`);
        }
      } catch (err) {
        console.error(`[gateway] Forward/reply failed:`, err.message);
      }
    }
  });
}

// ---------------------------------------------------------------------------
// Resolve agent name to UUID via OpenFang API
// ---------------------------------------------------------------------------
async function resolveAgentId(agentName) {
  if (resolvedAgentId) return resolvedAgentId;

  return new Promise((resolve, reject) => {
    const url = new URL(`${OPENFANG_URL}/api/agents`);
    const req = http.request(
      { hostname: url.hostname, port: url.port || 4200, path: url.pathname, method: 'GET', timeout: 10_000 },
      (res) => {
        let body = '';
        res.on('data', (chunk) => (body += chunk));
        res.on('end', () => {
          try {
            const agents = JSON.parse(body);
            const list = Array.isArray(agents) ? agents : agents.agents || [];
            const match = list.find((a) => a.name === agentName || a.id === agentName);
            if (match) {
              resolvedAgentId = match.id;
              console.log(`[gateway] Resolved agent "${agentName}" → ${match.id}`);
              resolve(match.id);
            } else {
              reject(new Error(`Agent "${agentName}" not found`));
            }
          } catch (e) {
            reject(new Error('Failed to parse agents list'));
          }
        });
      },
    );
    req.on('error', reject);
    req.on('timeout', () => { req.destroy(); reject(new Error('Agent resolve timeout')); });
    req.end();
  });
}

// ---------------------------------------------------------------------------
// Upload media to OpenFang API, return file_id
// ---------------------------------------------------------------------------
async function uploadToOpenFang(agentId, buffer, contentType, filename) {
  return new Promise((resolve, reject) => {
    const url = new URL(`${OPENFANG_URL}/api/agents/${encodeURIComponent(agentId)}/upload`);

    const req = http.request(
      {
        hostname: url.hostname,
        port: url.port || 4200,
        path: url.pathname,
        method: 'POST',
        headers: {
          'Content-Type': contentType,
          'Content-Length': buffer.length,
          'X-Filename': filename,
        },
        timeout: 30_000,
      },
      (res) => {
        let body = '';
        res.on('data', (chunk) => (body += chunk));
        res.on('end', () => {
          try {
            const data = JSON.parse(body);
            if (res.statusCode >= 400) {
              reject(new Error(`Upload failed (${res.statusCode}): ${data.error || body}`));
            } else {
              resolve(data.file_id || data.id || '');
            }
          } catch {
            reject(new Error(`Upload parse error: ${body}`));
          }
        });
      },
    );

    req.on('error', reject);
    req.on('timeout', () => { req.destroy(); reject(new Error('Upload timeout')); });
    req.write(buffer);
    req.end();
  });
}

// ---------------------------------------------------------------------------
// Forward incoming message to OpenFang API, return agent response
// ---------------------------------------------------------------------------
async function forwardToOpenFang(text, phone, pushName, metadata, attachments) {
  const agentId = await resolveAgentId(DEFAULT_AGENT_NAME);

  return new Promise((resolve, reject) => {
    const body = {
      message: text,
      metadata: metadata || {
        channel: 'whatsapp',
        sender: phone,
        sender_name: pushName,
      },
    };
    if (attachments && attachments.length > 0) {
      body.attachments = attachments;
    }
    const payload = JSON.stringify(body);

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
  });
}

// ---------------------------------------------------------------------------
// Send a message via Baileys (called by OpenFang for outgoing)
// ---------------------------------------------------------------------------
async function sendMessage(to, text) {
  if (!sock || connStatus !== 'connected') {
    throw new Error('WhatsApp not connected');
  }

  // If already a full JID (group or user), use as-is; otherwise normalize phone → JID
  let jid;
  if (to.includes('@')) {
    jid = to;
  } else {
    jid = to.replace(/^\+/, '') + '@s.whatsapp.net';
  }

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
  const pathname = url.pathname;

  try {
    // POST /login/start — start Baileys connection, return QR
    if (req.method === 'POST' && pathname === '/login/start') {
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
    if (req.method === 'GET' && pathname === '/login/status') {
      return jsonResponse(res, 200, {
        connected: connStatus === 'connected',
        message: statusMessage,
        expired: qrExpired,
      });
    }

    // POST /message/send — send outgoing message via Baileys
    if (req.method === 'POST' && pathname === '/message/send') {
      const body = await parseBody(req);
      const { to, text } = body;

      if (!to || !text) {
        return jsonResponse(res, 400, { error: 'Missing "to" or "text" field' });
      }

      await sendMessage(to, text);
      return jsonResponse(res, 200, { success: true, message: 'Sent' });
    }

    // GET /health — health check
    if (req.method === 'GET' && pathname === '/health') {
      return jsonResponse(res, 200, {
        status: 'ok',
        connected: connStatus === 'connected',
        session_id: sessionId || null,
      });
    }

    // 404
    jsonResponse(res, 404, { error: 'Not found' });
  } catch (err) {
    console.error(`[gateway] ${req.method} ${pathname} error:`, err.message);
    jsonResponse(res, 500, { error: err.message });
  }
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`[gateway] WhatsApp Web gateway listening on http://127.0.0.1:${PORT}`);
  console.log(`[gateway] OpenFang URL: ${OPENFANG_URL}`);
  console.log(`[gateway] Default agent: ${DEFAULT_AGENT_NAME}`);

  // Auto-connect if credentials already exist from a previous session
  const credsPath = path.join(__dirname, 'auth_store', 'creds.json');
  if (fs.existsSync(credsPath)) {
    console.log('[gateway] Found existing credentials — auto-connecting...');
    startConnection().catch((err) => {
      console.error('[gateway] Auto-connect failed:', err.message);
      statusMessage = 'Auto-connect failed. Use POST /login/start to retry.';
    });
  } else {
    console.log('[gateway] No credentials found. Waiting for POST /login/start to begin QR flow...');
  }
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
