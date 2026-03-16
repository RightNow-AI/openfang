#!/usr/bin/env node

import http from 'node:http';
import net from 'node:net';
import { randomUUID } from 'node:crypto';
import { execSync } from 'node:child_process';
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

// Resolve default agent: env var > config.toml [channels.whatsapp].default_agent > 'assistant'
function readDefaultAgentFromConfig() {
  const configPaths = ['/data/config.toml', path.join(__dirname, '..', 'config.toml')];
  for (const cfgPath of configPaths) {
    try {
      const content = fs.readFileSync(cfgPath, 'utf-8');
      // Find [channels.whatsapp] section and extract default_agent
      const waSection = content.match(/\[channels\.whatsapp\]([^[]*)/s);
      if (waSection) {
        const agentMatch = waSection[1].match(/default_agent\s*=\s*"([^"]+)"/);
        if (agentMatch) {
          console.log(`[gateway] Read default_agent="${agentMatch[1]}" from ${cfgPath}`);
          return agentMatch[1];
        }
      }
    } catch { /* file not found, try next */ }
  }
  return null;
}

let DEFAULT_AGENT = process.env.OPENFANG_DEFAULT_AGENT
  || readDefaultAgentFromConfig()
  || 'assistant';

// ---------------------------------------------------------------------------
// Resolve agent name → UUID if needed (OpenFang API requires UUID)
// ---------------------------------------------------------------------------
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

async function resolveAgentId(nameOrId) {
  if (UUID_RE.test(nameOrId)) return nameOrId;
  return new Promise((resolve) => {
    const url = new URL(`${OPENFANG_URL}/api/agents`);
    http.get({ hostname: url.hostname, port: url.port || 4200, path: url.pathname, timeout: 5000 }, (res) => {
      let body = '';
      res.on('data', (c) => (body += c));
      res.on('end', () => {
        try {
          const agents = JSON.parse(body);
          const match = agents.find((a) => a.name === nameOrId);
          if (match) {
            console.log(`[gateway] Resolved agent "${nameOrId}" → ${match.id}`);
            resolve(match.id);
          } else {
            console.warn(`[gateway] Agent "${nameOrId}" not found, using as-is`);
            resolve(nameOrId);
          }
        } catch {
          resolve(nameOrId);
        }
      });
    }).on('error', () => resolve(nameOrId));
  });
}

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
        setTimeout(async () => {
          try {
            await startConnection();
          } catch (err) {
            console.error(`[gateway] Reconnect attempt ${reconnectAttempt} failed:`, err.message);
            // Schedule another retry with increased backoff
            reconnectAttempt++;
            const nextDelay = Math.min(1000 * Math.pow(2, reconnectAttempt - 1), MAX_RECONNECT_DELAY);
            console.log(`[gateway] Will retry in ${nextDelay}ms (attempt ${reconnectAttempt})...`);
            statusMessage = `Reconnect failed, retrying (attempt ${reconnectAttempt})...`;
            setTimeout(async () => {
              try { await startConnection(); } catch (e) {
                console.error(`[gateway] Reconnect attempt ${reconnectAttempt} also failed:`, e.message);
              }
            }, nextDelay);
          }
        }, delay);
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

  // Debug: log key Baileys events
  for (const evt of ['messages.upsert', 'messages.update', 'messages.delete', 'message-receipt.update', 'messaging-history.set', 'chats.upsert', 'chats.update', 'contacts.upsert', 'contacts.update', 'presence.update', 'groups.upsert', 'groups.update']) {
    sock.ev.on(evt, (data) => {
      const count = Array.isArray(data) ? data.length : (data?.messages?.length || data?.chats?.length || '?');
      console.log(`[gateway] Event: ${evt} (count=${count})`);
    });
  }

  // Incoming messages → forward to OpenFang
  sock.ev.on('messages.upsert', async ({ messages, type }) => {
    console.log(`[gateway] messages.upsert: type=${type}, count=${messages.length}`);
    if (type !== 'notify') return;

    for (const msg of messages) {
      // Skip messages from self and status broadcasts
      if (msg.key.fromMe) continue;
      if (msg.key.remoteJid === 'status@broadcast') continue;

      // Send read receipt (blue checkmarks) immediately
      try {
        await sock.readMessages([msg.key]);
      } catch (err) {
        console.warn(`[gateway] Failed to send read receipt:`, err.message);
      }

      const remoteJid = msg.key.remoteJid || '';
      const isGroup = remoteJid.endsWith('@g.us');

      // Unwrap Baileys message wrappers (viewOnce, ephemeral, etc.)
      const rawMsg = msg.message;
      const m = rawMsg?.ephemeralMessage?.message
        || rawMsg?.viewOnceMessage?.message
        || rawMsg?.viewOnceMessageV2?.message
        || rawMsg?.viewOnceMessageV2Extension?.message
        || rawMsg?.documentWithCaptionMessage?.message
        || rawMsg;

      let text = m?.conversation
        || m?.extendedTextMessage?.text
        || '';

      // Debug: log message keys to diagnose undetected media
      const rawKeys = rawMsg ? Object.keys(rawMsg) : [];
      const unwrappedKeys = m ? Object.keys(m) : [];
      if (rawKeys.length > 0) {
        console.log(`[gateway] Message keys: raw=[${rawKeys.join(',')}] unwrapped=[${unwrappedKeys.join(',')}]`);
      }

      // Check for media messages
      const hasMedia = m?.imageMessage || m?.audioMessage || m?.videoMessage
        || m?.documentMessage || m?.stickerMessage;

      // Extract caption from media messages
      if (!text && hasMedia) {
        text = m?.imageMessage?.caption || m?.videoMessage?.caption || '';
      }

      // Download and upload media if present
      let attachments = [];
      if (hasMedia) {
        const mediaType = m?.imageMessage ? 'image' : m?.audioMessage ? 'audio' : m?.videoMessage ? 'video' : m?.documentMessage ? 'document' : 'sticker';
        const wasWrapped = m !== rawMsg;
        console.log(`[gateway] Media detected: type=${mediaType}, wrapped=${wasWrapped}, from=${msg.pushName || 'unknown'}`);
        const media = await downloadWhatsAppMedia(msg);
        if (media) {
          try {
            const fileId = await uploadMediaToOpenFang(media.buffer, media.mimetype, media.filename);
            attachments.push({
              file_id: fileId,
              filename: media.filename,
              content_type: media.mimetype,
            });
            // If no text/caption, describe what was sent
            if (!text) {
              const type = media.mimetype.split('/')[0]; // image, audio, video
              text = `[${type} attachment: ${media.filename}]`;
            }
          } catch (err) {
            console.error(`[gateway] Media upload failed:`, err.message);
            // Fallback to placeholder text
            if (!text) {
              if (m?.imageMessage) text = '[Image received - upload failed]';
              else if (m?.audioMessage) text = '[Voice note received - upload failed]';
              else if (m?.videoMessage) text = '[Video received - upload failed]';
              else if (m?.documentMessage) text = '[Document received - upload failed]';
              else if (m?.stickerMessage) text = '[Sticker received - upload failed]';
            }
          }
        } else if (!text) {
          // Media download failed, use placeholder
          if (m?.imageMessage) text = '[Image received - download failed]';
          else if (m?.audioMessage) text = '[Voice note received - download failed]';
          else if (m?.videoMessage) text = '[Video received - download failed]';
          else if (m?.documentMessage) text = '[Document received - download failed]';
          else if (m?.stickerMessage) text = '[Sticker received - download failed]';
        }
      }

      // Skip truly empty messages (no text, no media)
      if (!text && attachments.length === 0) continue;

      // For groups: real sender is in participant; for DMs: it's remoteJid
      const senderJid = isGroup ? (msg.key.participant || '') : remoteJid;
      const phone = '+' + senderJid.replace(/@.*$/, '');
      const pushName = msg.pushName || phone;

      const metadata = {
        channel: 'whatsapp',
        sender: phone,
        sender_name: pushName,
      };
      if (isGroup) {
        metadata.group_jid = remoteJid;
        metadata.is_group = true;
        console.log(`[gateway] Group msg from ${pushName} (${phone}) in ${remoteJid}: ${text.substring(0, 80)}`);
      } else {
        console.log(`[gateway] Incoming from ${pushName} (${phone}): ${text.substring(0, 80)}`);
      }

      // Forward to OpenFang agent
      try {
        const response = await forwardToOpenFang(text, phone, pushName, metadata, attachments);
        if (response && sock) {
          // Reply in the same context: group → group, DM → DM
          // Use remoteJid directly to preserve @lid JIDs (WhatsApp Linked Identity)
          const replyJid = remoteJid;
          await sock.sendMessage(replyJid, { text: markdownToWhatsApp(response) });
          console.log(`[gateway] Replied to ${pushName}${isGroup ? ' in group ' + remoteJid : ''}`);
        }
      } catch (err) {
        console.error(`[gateway] Forward/reply failed:`, err.message);
      }
    }
  });
}

// ---------------------------------------------------------------------------
// Upload media to OpenFang, return file_id
// ---------------------------------------------------------------------------
async function uploadMediaToOpenFang(buffer, contentType, filename) {
  return new Promise((resolve, reject) => {
    const url = new URL(`${OPENFANG_URL}/api/agents/${encodeURIComponent(DEFAULT_AGENT)}/upload`);

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
            if (data.file_id) {
              console.log(`[gateway] Uploaded media: ${filename} → ${data.file_id}`);
              resolve(data.file_id);
            } else {
              reject(new Error(`Upload failed: ${body}`));
            }
          } catch {
            reject(new Error(`Upload parse error: ${body}`));
          }
        });
      },
    );

    req.on('error', reject);
    req.on('timeout', () => {
      req.destroy();
      reject(new Error('Upload timeout'));
    });
    req.write(buffer);
    req.end();
  });
}

// ---------------------------------------------------------------------------
// Download media from a WhatsApp message, returns { buffer, mimetype, filename }
// ---------------------------------------------------------------------------
async function downloadWhatsAppMedia(msg) {
  // Unwrap Baileys message wrappers (viewOnce, ephemeral, etc.)
  const rawMsg = msg.message;
  const m = rawMsg?.ephemeralMessage?.message
    || rawMsg?.viewOnceMessage?.message
    || rawMsg?.viewOnceMessageV2?.message
    || rawMsg?.viewOnceMessageV2Extension?.message
    || rawMsg?.documentWithCaptionMessage?.message
    || rawMsg;
  let mediaMsg = null;
  let mimetype = 'application/octet-stream';
  let filename = 'file';

  if (m?.imageMessage) {
    mediaMsg = m.imageMessage;
    mimetype = mediaMsg.mimetype || 'image/jpeg';
    filename = `image_${Date.now()}.${mimetype.split('/')[1] || 'jpg'}`;
  } else if (m?.audioMessage) {
    mediaMsg = m.audioMessage;
    mimetype = mediaMsg.mimetype || 'audio/ogg';
    const ext = mediaMsg.ptt ? 'ogg' : (mimetype.split('/')[1] || 'ogg');
    filename = `audio_${Date.now()}.${ext}`;
  } else if (m?.videoMessage) {
    mediaMsg = m.videoMessage;
    mimetype = mediaMsg.mimetype || 'video/mp4';
    filename = `video_${Date.now()}.${mimetype.split('/')[1] || 'mp4'}`;
  } else if (m?.documentMessage) {
    mediaMsg = m.documentMessage;
    mimetype = mediaMsg.mimetype || 'application/octet-stream';
    filename = mediaMsg.fileName || `document_${Date.now()}`;
  } else if (m?.stickerMessage) {
    mediaMsg = m.stickerMessage;
    mimetype = mediaMsg.mimetype || 'image/webp';
    filename = `sticker_${Date.now()}.webp`;
  }

  if (!mediaMsg) return null;

  try {
    // For wrapped messages, create a shallow copy with unwrapped .message
    // so Baileys' downloadMediaMessage can find the media content
    const downloadMsg = (m !== rawMsg) ? { ...msg, message: m } : msg;
    const buffer = await downloadMediaMessage(downloadMsg, 'buffer', {});
    return { buffer, mimetype, filename };
  } catch (err) {
    console.error(`[gateway] Media download failed:`, err.message);
    return null;
  }
}

// ---------------------------------------------------------------------------
// Forward incoming message to OpenFang API, return agent response
// ---------------------------------------------------------------------------
function forwardToOpenFang(text, phone, pushName, metadata, attachments) {
  return new Promise((resolve, reject) => {
    const body = {
      message: text,
      sender_id: phone,
      sender_name: pushName,
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

    const url = new URL(`${OPENFANG_URL}/api/agents/${encodeURIComponent(DEFAULT_AGENT)}/message`);

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
        timeout: 300_000, // LLM calls can be very slow (5 min)
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
// Convert Markdown to WhatsApp-compatible formatting
// ---------------------------------------------------------------------------
function markdownToWhatsApp(text) {
  if (!text) return text;

  // Protect code blocks from transformation (``` ... ```)
  const codeBlocks = [];
  text = text.replace(/```[\s\S]*?```/g, (match) => {
    codeBlocks.push(match);
    return `\x00CB${codeBlocks.length - 1}\x00`;
  });

  // Protect inline code (`...`)
  const inlineCode = [];
  text = text.replace(/`[^`]+`/g, (match) => {
    inlineCode.push(match);
    return `\x00IC${inlineCode.length - 1}\x00`;
  });

  // Headers: # Title → *Title* (bold in WhatsApp)
  text = text.replace(/^#{1,6}\s+(.+)$/gm, '*$1*');

  // Bold: **text** → *text*
  text = text.replace(/\*\*(.+?)\*\*/g, '*$1*');

  // Strikethrough: ~~text~~ → ~text~
  text = text.replace(/~~(.+?)~~/g, '~$1~');

  // Links: [text](url) → text (url)
  text = text.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '$1 ($2)');

  // Images: ![alt](url) → alt (url)
  text = text.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, '$1 ($2)');

  // Horizontal rules: --- or *** or ___ → ———
  text = text.replace(/^[-*_]{3,}$/gm, '———');

  // Restore inline code
  text = text.replace(/\x00IC(\d+)\x00/g, (_, i) => inlineCode[i]);

  // Restore code blocks
  text = text.replace(/\x00CB(\d+)\x00/g, (_, i) => codeBlocks[i]);

  return text;
}

// ---------------------------------------------------------------------------
// Send a message via Baileys (called by OpenFang for outgoing)
// ---------------------------------------------------------------------------
async function sendMessage(to, text) {
  if (!sock || connStatus !== 'connected') {
    throw new Error('WhatsApp not connected');
  }

  // If already a full JID (group or user), use as-is; otherwise normalize phone → JID
  const jid = to.includes('@') ? to : to.replace(/^\+/, '') + '@s.whatsapp.net';

  await sock.sendMessage(jid, { text: markdownToWhatsApp(text) });
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

// ---------------------------------------------------------------------------
// PID file management
// ---------------------------------------------------------------------------
const PID_FILE = path.join(__dirname, 'gateway.pid');

function writePidFile() {
  fs.writeFileSync(PID_FILE, String(process.pid), 'utf-8');
  console.log(`[gateway] PID file written: ${PID_FILE} (${process.pid})`);
}

function cleanupPidFile() {
  try { fs.unlinkSync(PID_FILE); } catch {}
}

function killStalePidFile() {
  try {
    const oldPid = parseInt(fs.readFileSync(PID_FILE, 'utf-8').trim(), 10);
    if (oldPid && oldPid !== process.pid) {
      try {
        process.kill(oldPid, 0); // check if alive
        console.warn(`[gateway] Stale PID file found (PID ${oldPid}), killing...`);
        process.kill(oldPid, 'SIGTERM');
        // Wait briefly, then force-kill if still alive
        const start = Date.now();
        while (Date.now() - start < 2000) {
          try { process.kill(oldPid, 0); } catch { break; } // dead
          execSync('sleep 0.2');
        }
        try { process.kill(oldPid, 'SIGKILL'); } catch {}
      } catch {
        // Process already dead — clean up stale PID file
      }
    }
  } catch {
    // No PID file or unreadable — fine
  }
}

// ---------------------------------------------------------------------------
// Port cleanup — robust cleanup with PID file + port scan + retry
// ---------------------------------------------------------------------------
function isPortFree(port) {
  return new Promise((resolve) => {
    const testSock = net.createConnection({ port, host: '127.0.0.1' }, () => {
      testSock.destroy();
      resolve(false); // occupied
    });
    testSock.on('error', () => resolve(true)); // free
  });
}

function killProcessOnPort(port) {
  try {
    const out = execSync(`ss -tlnp sport = :${port} 2>/dev/null || true`, { encoding: 'utf-8' });
    const pidMatch = out.match(/pid=(\d+)/);
    if (pidMatch) {
      const pid = parseInt(pidMatch[1], 10);
      if (pid !== process.pid) {
        console.warn(`[gateway] Killing process PID ${pid} on port ${port}`);
        try { process.kill(pid, 'SIGTERM'); } catch {}
        return pid;
      }
    }
  } catch {}
  return null;
}

async function ensurePortFree(port, maxRetries = 3) {
  // Step 1: Kill stale process from PID file
  killStalePidFile();

  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    if (await isPortFree(port)) {
      return; // port is free
    }

    console.warn(`[gateway] Port ${port} occupied (attempt ${attempt}/${maxRetries})`);

    // Try to kill whatever is holding the port
    const pid = killProcessOnPort(port);

    if (pid) {
      // Wait for process to die
      await new Promise((r) => setTimeout(r, 1500));
      // Force-kill if still alive
      try { process.kill(pid, 0); process.kill(pid, 'SIGKILL'); } catch {}
      await new Promise((r) => setTimeout(r, 500));
    } else {
      // ss couldn't find it — wait and retry
      await new Promise((r) => setTimeout(r, 1000));
    }
  }

  // Final check
  if (!(await isPortFree(port))) {
    console.error(`[gateway] FATAL: Port ${port} still occupied after ${maxRetries} cleanup attempts`);
    process.exit(1);
  }
}

await ensurePortFree(PORT);
writePidFile();

// Listen with EADDRINUSE retry as last-resort safety net
function startServer() {
  return new Promise((resolve, reject) => {
    server.once('error', (err) => {
      if (err.code === 'EADDRINUSE') {
        console.warn(`[gateway] EADDRINUSE on listen — retrying cleanup...`);
        ensurePortFree(PORT, 2).then(() => {
          server.listen(PORT, '127.0.0.1', resolve);
        }).catch(reject);
      } else {
        reject(err);
      }
    });
    server.listen(PORT, '127.0.0.1', resolve);
  });
}

await startServer();

// Post-listen setup
(async () => {
  // Resolve agent name to UUID before anything else
  DEFAULT_AGENT = await resolveAgentId(DEFAULT_AGENT);
  console.log(`[gateway] WhatsApp Web gateway listening on http://127.0.0.1:${PORT}`);
  console.log(`[gateway] OpenFang URL: ${OPENFANG_URL}`);
  console.log(`[gateway] Default agent: ${DEFAULT_AGENT}`);

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
})();

// ---------------------------------------------------------------------------
// Health check — detect zombie connections (socket exists but not functional)
// ---------------------------------------------------------------------------
const HEALTH_CHECK_INTERVAL = 30_000; // 30 seconds
let lastMessageTime = Date.now();

// Track last successful activity
const originalOnMessage = null;

setInterval(() => {
  if (connStatus !== 'connected' || !sock) return;

  // Check if the socket's underlying WebSocket is still alive
  try {
    const wsState = sock?.ws?.readyState;
    // WebSocket states: 0=CONNECTING, 1=OPEN, 2=CLOSING, 3=CLOSED
    if (wsState !== undefined && wsState !== 1) {
      console.warn(`[gateway] Health check: WebSocket in bad state (${wsState}), triggering reconnect`);
      connStatus = 'disconnected';
      statusMessage = 'Health check detected stale connection, reconnecting...';
      reconnectAttempt = 0;
      try { sock.end(); } catch {}
      sock = null;
      startConnection().catch((err) => {
        console.error('[gateway] Health-check reconnect failed:', err.message);
      });
    }
  } catch (err) {
    console.warn(`[gateway] Health check error:`, err.message);
  }
}, HEALTH_CHECK_INTERVAL);

// ---------------------------------------------------------------------------
// Uncaught exception / rejection handlers — prevent silent death
// ---------------------------------------------------------------------------
process.on('uncaughtException', (err) => {
  console.error('[gateway] UNCAUGHT EXCEPTION:', err.message, err.stack);
  // Don't exit — PM2 will restart, but let's try to self-heal first
  if (connStatus === 'connected') {
    connStatus = 'disconnected';
    statusMessage = 'Recovering from uncaught exception...';
    setTimeout(async () => {
      try { await startConnection(); } catch (e) {
        console.error('[gateway] Recovery after uncaught exception failed:', e.message);
      }
    }, 5000);
  }
});

process.on('unhandledRejection', (reason) => {
  console.error('[gateway] UNHANDLED REJECTION:', reason);
});

// Graceful shutdown — clean up PID file
process.on('SIGINT', () => {
  console.log('\n[gateway] Shutting down...');
  cleanupPidFile();
  if (sock) sock.end();
  server.close(() => process.exit(0));
});

process.on('SIGTERM', () => {
  cleanupPidFile();
  if (sock) sock.end();
  server.close(() => process.exit(0));
});

process.on('exit', () => {
  cleanupPidFile();
});
