#!/usr/bin/env node
/**
 * scripts/heartbeat.js
 *
 * Heartbeat-driven task orchestration runner for OpenFang.
 *
 * Task types:
 *   short  — executes inline in the main loop (health checks, file probes)
 *   ai     — routes through OpenFang daemon agent API (real AI subagent loops)
 *   long   — spawns worker_threads for CPU-bound shell jobs (cargo, cypress)
 *
 * AI task rule:
 *   AI reasoning / multi-step subagents -> OpenFang daemon (runAiTask)
 *   CPU-heavy local execution           -> worker_threads  (spawnLongTaskWorker)
 *
 * AI tasks persist state to .heartbeat-state.json so remoteRunId and retry
 * count survive process restarts.
 *
 * Usage:
 *   node scripts/heartbeat.js
 *   node scripts/heartbeat.js --verbose
 *   node scripts/heartbeat.js --dry-run
 *   node scripts/heartbeat.js --task T010
 */

'use strict';

const fs = require('fs');
const path = require('path');
const http = require('http');
const crypto = require('crypto');
const { spawn } = require('child_process');
const { Worker, isMainThread, parentPort, workerData } = require('worker_threads');

// ─── Config ───────────────────────────────────────────────────────────────────

const ROOT = path.resolve(__dirname, '..');
const HEARTBEAT_PATH = path.join(ROOT, 'HEARTBEAT.md');
const STATE_PATH = path.join(ROOT, '.heartbeat-state.json');
const NEXTJS_DIR = path.join(ROOT, 'sdk', 'javascript', 'examples', 'nextjs-app-router');
const API_BASE = 'http://127.0.0.1:50051';
const NEXT_BASE = 'http://localhost:3002';
const ENV_FILE = path.join(ROOT, '.env');

// AI task settings
const AI_DEFAULT_AGENT = 'alive';   // daemon agent name for subagent tasks
const AI_TIMEOUT_MS = 120_000;      // 2 minutes per AI call
const AI_MAX_RETRIES = 3;           // max attempts before marking failed
const AI_RETRY_BASE_MS = 1_500;     // exponential backoff base delay

const args = process.argv.slice(2);
const VERBOSE = args.includes('--verbose');
const DRY_RUN = args.includes('--dry-run');
const FORCE_TASK = (() => {
  const idx = args.indexOf('--task');
  return idx !== -1 ? args[idx + 1] : null;
})();

// ─── Worker thread: long-task executor ────────────────────────────────────────

if (!isMainThread) {
  const { task, verbose } = workerData;
  runLongTask(task, verbose)
    .then((result) => parentPort.postMessage({ ok: true, taskId: task.id, result }))
    .catch((err) => parentPort.postMessage({ ok: false, taskId: task.id, error: err.message }));
  return; // Worker exits after posting
}

// ─── Main thread ──────────────────────────────────────────────────────────────

main().catch((err) => {
  log(`[HEARTBEAT] Fatal error: ${err.message}`);
  process.exit(1);
});

async function main() {
  // Load .env into process.env (best-effort)
  if (fs.existsSync(ENV_FILE)) {
    const lines = fs.readFileSync(ENV_FILE, 'utf8').split('\n');
    for (const line of lines) {
      const m = line.match(/^([A-Z_][A-Z0-9_]*)=(.*)$/);
      if (m && !process.env[m[1]]) {
        process.env[m[1]] = m[2].replace(/^["']|["']$/g, '');
      }
    }
  }

  // Read and validate HEARTBEAT.md
  if (!fs.existsSync(HEARTBEAT_PATH)) {
    console.error('HEARTBEAT_ERROR: HEARTBEAT.md not found at', HEARTBEAT_PATH);
    process.exit(2);
  }

  let heartbeatSrc = fs.readFileSync(HEARTBEAT_PATH, 'utf8');
  const tasks = parseTaskRegistry(heartbeatSrc);

  if (!tasks.length) {
    log('[HEARTBEAT] No tasks found in registry.');
    console.log('HEARTBEAT_OK');
    return;
  }

  // Filter to pending tasks (or a specific forced task)
  const pending = tasks.filter((t) =>
    FORCE_TASK ? t.id === FORCE_TASK : t.status === 'pending',
  );

  if (!pending.length) {
    log('[HEARTBEAT] No pending tasks. Nothing to do.');
    console.log('HEARTBEAT_OK');
    return;
  }

  // Sort by priority ascending
  pending.sort((a, b) => a.priority - b.priority);

  log(`[HEARTBEAT] Found ${pending.length} pending tasks.`);
  if (DRY_RUN) {
    log('[HEARTBEAT] DRY-RUN mode — classifying tasks only, no execution.\n');
    for (const t of pending) {
      const typeLabel = t.type === 'ai' ? 'AI   ' : t.type === 'short' ? 'SHORT' : 'LONG ';
      log(`  [${typeLabel}] ${t.id} (p${t.priority}) — ${t.title}`);
    }
    console.log('\nHEARTBEAT_OK');
    return;
  }

  const backgroundPromises = []; // Promises for ai and long tasks

  for (const task of pending) {
    heartbeatSrc = markTaskStatus(heartbeatSrc, task.id, 'in-progress');
    writeSrc(heartbeatSrc);

    if (task.type === 'short') {
      // ── Short: run inline, block main loop ─────────────────────────────────
      log(`[SHORT] Executing: ${task.id} — ${task.title}`);
      const result = await runShortTask(task);
      const outcome = result.ok ? 'done' : 'failed';
      heartbeatSrc = markTaskStatus(heartbeatSrc, task.id, outcome);
      heartbeatSrc = appendLog(heartbeatSrc, task.id, outcome, result.message);
      writeSrc(heartbeatSrc);
      log(`[SHORT] ${task.id} → ${outcome}: ${result.message}`);

    } else if (task.type === 'ai') {
      // ── AI: send to OpenFang daemon agent — no worker_threads ───────────────
      log(`[AI   ] Delegating to daemon agent "${AI_DEFAULT_AGENT}": ${task.id} — ${task.title}`);
      const promise = runAiTaskWithRetry(task).then((result) => {
        let src = fs.readFileSync(HEARTBEAT_PATH, 'utf8');
        const outcome = result.ok ? 'done' : 'failed';
        src = markTaskStatus(src, task.id, outcome);
        src = appendLog(src, task.id, outcome, result.message ?? result.error ?? '');
        writeSrc(src);
        log(`[AI   ] ${task.id} → ${outcome}: ${result.message ?? result.error ?? ''}`);
      });
      backgroundPromises.push(promise);
      // Do NOT await — main loop continues immediately

    } else {
      // ── Long: CPU-bound shell job → worker_threads ─────────────────────────
      log(`[LONG ] Spawning worker for: ${task.id} — ${task.title}`);
      const promise = spawnLongTaskWorker(task, heartbeatSrc).then((result) => {
        let src = fs.readFileSync(HEARTBEAT_PATH, 'utf8');
        const outcome = result.ok ? 'done' : 'failed';
        src = markTaskStatus(src, task.id, outcome);
        src = appendLog(src, task.id, outcome, result.message ?? result.error ?? '');
        writeSrc(src);
        log(`[LONG ] ${task.id} → ${outcome}: ${result.message ?? result.error ?? ''}`);
      });
      backgroundPromises.push(promise);
      // Do NOT await — main loop continues immediately
    }
  }

  // Main loop tasks done — emit HEARTBEAT_OK immediately
  console.log('HEARTBEAT_OK');
  log(`[HEARTBEAT] Main loop complete. ${backgroundPromises.length} background task(s) running.`);

  // Await background tasks so process doesn't exit before they write results
  if (backgroundPromises.length) {
    await Promise.allSettled(backgroundPromises);
    log('[HEARTBEAT] All background tasks finished.');
  }
}

// ─── AI task runner ───────────────────────────────────────────────────────────
// Routes AI-type tasks through the OpenFang daemon agent instead of worker_threads.
// Persists state (remoteRunId, retries, status) to .heartbeat-state.json.

/**
 * Run an AI task against the daemon with retry + timeout logic.
 * Uses POST /api/agents/:agentId/message (synchronous — daemon has no runs API yet).
 *
 * @param {object} task
 * @returns {Promise<{ ok: boolean; message: string }>}
 */
async function runAiTaskWithRetry(task) {
  const state = loadAiState();
  const existing = state[task.id] ?? {};
  const agentId = AI_DEFAULT_AGENT;
  const remoteRunId = existing.remoteRunId ?? crypto.randomUUID();
  const retriesDone = existing.retries ?? 0;

  // Save initial state so remoteRunId is visible immediately in logs
  saveAiState({
    ...state,
    [task.id]: {
      taskId: task.id,
      agent: agentId,
      remoteRunId,
      status: 'running',
      startedAt: existing.startedAt ?? new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      retries: retriesDone,
    },
  });

  log(`[AI   ] remoteRunId=${remoteRunId} agent=${agentId} (attempt ${retriesDone + 1}/${AI_MAX_RETRIES})`);

  // Build prompt from task fields
  const prompt = buildAiPrompt(task);

  for (let attempt = 1; attempt <= AI_MAX_RETRIES; attempt++) {
    if (attempt > 1) {
      const delay = AI_RETRY_BASE_MS * Math.pow(2, attempt - 2);
      log(`[AI   ] ${task.id} retry ${attempt}/${AI_MAX_RETRIES} — waiting ${delay}ms`);
      await sleep(delay);
    }

    try {
      const result = await httpPost(
        `${API_BASE}/api/agents/${encodeURIComponent(agentId)}/message`,
        { message: prompt },
        AI_TIMEOUT_MS,
      );

      if (!result.ok) {
        throw new Error(`HTTP ${result.status}: ${result.body.slice(0, 200)}`);
      }

      let parsed;
      try { parsed = JSON.parse(result.body); } catch {
        throw new Error(`Non-JSON response: ${result.body.slice(0, 200)}`);
      }

      if (parsed.error) throw new Error(`Daemon error: ${parsed.error}`);

      const summary = String(parsed.response ?? '').slice(0, 300);

      saveAiState({
        ...loadAiState(),
        [task.id]: {
          taskId: task.id,
          agent: agentId,
          remoteRunId,
          status: 'completed',
          startedAt: existing.startedAt ?? new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          retries: retriesDone + attempt - 1,
          output: parsed,
        },
      });

      log(`[AI   ] ${task.id} completed — ${parsed.iterations} iter, $${parsed.cost_usd?.toFixed(4) ?? '?'}`);
      return { ok: true, message: `Agent response (${parsed.input_tokens}in/${parsed.output_tokens}out): ${summary}` };

    } catch (err) {
      const isLast = attempt === AI_MAX_RETRIES;
      log(`[AI   ] ${task.id} attempt ${attempt} failed: ${err.message}`);

      if (isLast) {
        saveAiState({
          ...loadAiState(),
          [task.id]: {
            taskId: task.id,
            agent: agentId,
            remoteRunId,
            status: 'failed',
            startedAt: existing.startedAt ?? new Date().toISOString(),
            updatedAt: new Date().toISOString(),
            retries: retriesDone + attempt,
            error: err.message,
          },
        });
        return { ok: false, message: `Failed after ${attempt} attempt(s): ${err.message}` };
      }
    }
  }

  // Unreachable, but TypeScript-style exhaustiveness
  return { ok: false, message: 'Unexpected exit from retry loop' };
}

/**
 * Build the prompt string an AI agent receives.
 * Embeds task context and success criteria so the agent has full visibility.
 *
 * @param {object} task
 * @returns {string}
 */
function buildAiPrompt(task) {
  return [
    `Task: ${task.title}`,
    task.context ? `\nContext:\n${task.context}` : '',
    task.success_criteria ? `\nSuccess Criteria:\n${task.success_criteria}` : '',
    '\nWork this task autonomously. Return structured observations and a final conclusion.',
  ].join('');
}

// ─── AI state persistence ─────────────────────────────────────────────────────

function loadAiState() {
  try {
    if (!fs.existsSync(STATE_PATH)) return {};
    return JSON.parse(fs.readFileSync(STATE_PATH, 'utf8'));
  } catch {
    return {};
  }
}

function saveAiState(state) {
  try {
    fs.writeFileSync(STATE_PATH, JSON.stringify(state, null, 2), 'utf8');
  } catch {
    // Non-fatal
  }
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// ─── Short task implementations ───────────────────────────────────────────────

async function runShortTask(task) {
  try {
    switch (task.id) {
      case 'T001': return await checkApiHealth();
      case 'T002': return await checkNextjsHealth();
      case 'T003': return await checkAgentCount();
      case 'T008': return checkNextjsBuildId();
      case 'T009': return tailDaemonLog();
      default:
        return { ok: false, message: `No short-task handler for ${task.id}` };
    }
  } catch (err) {
    return { ok: false, message: `Exception: ${err.message}` };
  }
}

async function checkApiHealth() {
  const res = await httpGet(`${API_BASE}/api/health`);
  if (!res.ok) return { ok: false, message: `HTTP ${res.status} from ${API_BASE}/api/health` };
  let body;
  try { body = JSON.parse(res.body); } catch { return { ok: false, message: 'Non-JSON health response' }; }
  if (!body.status) return { ok: false, message: `Health response missing 'status' field: ${res.body}` };
  return { ok: true, message: `Daemon healthy — status="${body.status}"` };
}

async function checkNextjsHealth() {
  const res = await httpGet(`${NEXT_BASE}/`);
  if (!res.ok) return { ok: false, message: `HTTP ${res.status} from Next.js at ${NEXT_BASE}` };
  if (!res.body.includes('<!DOCTYPE html') && !res.body.includes('<!doctype html')) {
    return { ok: false, message: 'Response did not contain <!DOCTYPE html>' };
  }
  return { ok: true, message: `Next.js reachable at ${NEXT_BASE}` };
}

async function checkAgentCount() {
  const res = await httpGet(`${API_BASE}/api/agents`);
  if (!res.ok) return { ok: false, message: `HTTP ${res.status} from /api/agents` };
  let agents;
  try { agents = JSON.parse(res.body); } catch { return { ok: false, message: 'Non-JSON /api/agents response' }; }
  if (!Array.isArray(agents) || agents.length === 0) {
    return { ok: false, message: `No agents registered (got: ${res.body.slice(0, 80)})` };
  }
  return { ok: true, message: `${agents.length} agents registered: ${agents.map((a) => a.id).join(', ')}` };
}

function checkNextjsBuildId() {
  const buildIdPath = path.join(NEXTJS_DIR, '.next', 'BUILD_ID');
  if (!fs.existsSync(buildIdPath)) {
    return { ok: false, message: '.next/BUILD_ID not found — run `npm run build` first' };
  }
  const id = fs.readFileSync(buildIdPath, 'utf8').trim();
  return { ok: true, message: `Next.js build exists (BUILD_ID: ${id})` };
}

function tailDaemonLog() {
  const logPath = path.join(process.env.TEMP || '/tmp', 'of-stderr.txt');
  if (!fs.existsSync(logPath)) {
    return { ok: true, message: 'No daemon log found at $TEMP\\of-stderr.txt (daemon may not have been started via Start-Process redirect)' };
  }
  const lines = fs.readFileSync(logPath, 'utf8').split('\n').filter(Boolean);
  const tail = lines.slice(-20).join('\n');
  const errors = lines.filter((l) => /error|panic/i.test(l));
  if (errors.length) {
    return { ok: false, message: `Daemon log has ${errors.length} error/panic lines.\nLast 20:\n${tail}` };
  }
  return { ok: true, message: `Daemon log clean (${lines.length} total lines).\nLast 20:\n${tail}` };
}

// ─── Long task implementations (run inside worker thread) ─────────────────────

async function runLongTask(task, verbose) {
  switch (task.id) {
    case 'T004': return runCypress(verbose);
    case 'T005': return runCargoCommand('cargo build --workspace --lib', task, verbose);
    case 'T006': return runCargoCommand('cargo test --workspace', task, verbose);
    case 'T007': return runCargoCommand('cargo clippy --workspace --all-targets -- -D warnings', task, verbose);
    default:
      return { ok: false, message: `No long-task handler for ${task.id}` };
  }
}

async function runCypress(verbose) {
  // Cypress requires both daemon (:50051) and Next.js (:3002) to be running.
  // We do a best-effort health check before running, then report if they're down.
  const apiOk = await httpGet(`${API_BASE}/api/health`).then((r) => r.ok).catch(() => false);
  const nextOk = await httpGet(`${NEXT_BASE}/`).then((r) => r.ok).catch(() => false);
  if (!apiOk || !nextOk) {
    const missing = [!apiOk && 'API daemon (:50051)', !nextOk && 'Next.js (:3002)'].filter(Boolean).join(', ');
    return {
      ok: false,
      message: `Prerequisites not running: ${missing}. Start them before running Cypress.`,
    };
  }

  return new Promise((resolve) => {
    const child = spawn(
      process.platform === 'win32' ? 'cmd' : 'sh',
      process.platform === 'win32'
        ? ['/c', 'npm run cy:run:headless']
        : ['-c', 'npm run cy:run:headless'],
      { cwd: NEXTJS_DIR, env: { ...process.env }, shell: false },
    );

    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (d) => {
      stdout += d;
      if (verbose) process.stdout.write(`[T004:cypress] ${d}`);
    });
    child.stderr.on('data', (d) => {
      stderr += d;
      if (verbose) process.stderr.write(`[T004:cypress] ${d}`);
    });

    child.on('close', (code) => {
      const passingMatch = stdout.match(/(\d+) passing/);
      const failingMatch = stdout.match(/(\d+) failing/);
      const passing = passingMatch ? parseInt(passingMatch[1], 10) : 0;
      const failing = failingMatch ? parseInt(failingMatch[1], 10) : 0;

      if (code === 0) {
        resolve({ ok: true, message: `Cypress: ${passing} passing, ${failing} failing. All specs passed.` });
      } else {
        // Extract first error per failing spec
        const errorLines = stdout
          .split('\n')
          .filter((l) => /^\s+\d+\)/.test(l) || /AssertionError|Error:/.test(l))
          .slice(0, 15)
          .join('\n');
        resolve({
          ok: false,
          message: `Cypress failed (exit ${code}): ${passing} passing, ${failing} failing.\n${errorLines}`,
        });
      }
    });
  });
}

async function runCargoCommand(cmd, task, verbose) {
  return new Promise((resolve) => {
    const [bin, ...cmdArgs] = cmd.split(' ');
    const child = spawn(bin, cmdArgs, {
      cwd: ROOT,
      env: { ...process.env },
      shell: process.platform === 'win32',
    });

    let output = '';
    const gather = (d) => {
      output += d;
      if (verbose) process.stdout.write(`[${task.id}] ${d}`);
    };
    child.stdout.on('data', gather);
    child.stderr.on('data', gather);

    child.on('close', (code) => {
      if (code === 0) {
        const warnings = (output.match(/^warning\[/gm) || []).length;
        resolve({ ok: true, message: `Exit 0. ${warnings} warning(s).` });
      } else {
        const errors = output
          .split('\n')
          .filter((l) => /^error/.test(l))
          .slice(0, 10)
          .join('\n');
        resolve({ ok: false, message: `Exit ${code}.\nFirst errors:\n${errors}` });
      }
    });
  });
}

// ─── Worker spawner ───────────────────────────────────────────────────────────

function spawnLongTaskWorker(task) {
  return new Promise((resolve) => {
    const worker = new Worker(__filename, {
      workerData: { task, verbose: VERBOSE },
    });
    worker.on('message', (msg) => resolve(msg));
    worker.on('error', (err) => resolve({ ok: false, taskId: task.id, error: err.message }));
    worker.on('exit', (code) => {
      if (code !== 0) resolve({ ok: false, taskId: task.id, error: `Worker exited with code ${code}` });
    });
  });
}

// ─── HEARTBEAT.md parser ──────────────────────────────────────────────────────

function parseTaskRegistry(src) {
  // Use regex with ^ anchor so we only match the H2 heading at line start,
  // not any inline backtick references to the section name earlier in the doc.
  const parts = src.split(/^## TASK REGISTRY$/m);
  const registrySection = parts[parts.length - 1]; // always take the last segment
  if (!registrySection || registrySection.trim().length === 0) return [];

  const taskBlocks = registrySection.split(/^### (T\d+)/m);
  const tasks = [];

  // taskBlocks: ['preamble', 'T001', 'body', 'T002', 'body', ...]
  for (let i = 1; i < taskBlocks.length; i += 2) {
    const id = taskBlocks[i].trim();
    const body = taskBlocks[i + 1] || '';

    // Parse all `- **key**: value` lines at once.
    // Use [*][*] instead of \*\* to avoid new RegExp() escape issues with CRLF files.
    const attrs = {};
    const attrRe = /[*][*](\w[\w_-]*)[*][*]:\s*([^\r\n]+)/g;
    let m;
    while ((m = attrRe.exec(body)) !== null) {
      attrs[m[1]] = m[2].trim();
    }
    const get = (key) => attrs[key] || null;

    tasks.push({
      id,
      status: get('status') || 'pending',
      type: get('type') || 'short',
      priority: parseInt(get('priority') || '5', 10),
      title: get('title') || id,
      context: get('context') || '',
      success_criteria: get('success_criteria') || '',
    });
  }

  return tasks;
}

function markTaskStatus(src, taskId, newStatus) {
  // Replace `- **status**: <old>` inside the block for this task ID
  const re = new RegExp(
    `(### ${taskId}[\\s\\S]*?)(- \\*\\*status\\*\\*: \\w+)`,
    'm',
  );
  return src.replace(re, (_, prefix, statusLine) =>
    `${prefix}- **status**: ${newStatus}`,
  );
}

function appendLog(src, taskId, outcome, message) {
  const timestamp = new Date().toISOString();
  const entry = `\n- **${timestamp}** — ${taskId} → ${outcome}: ${message.replace(/\n/g, ' ').slice(0, 200)}`;
  return src.replace('## HEARTBEAT_LOG', `## HEARTBEAT_LOG${entry}`);
}

function writeSrc(src) {
  fs.writeFileSync(HEARTBEAT_PATH, src, 'utf8');
}

// ─── HTTP helpers ─────────────────────────────────────────────────────────────

function httpGet(url) {
  const { request } = url.startsWith('https') ? require('https') : require('http');
  return new Promise((resolve) => {
    const req = request(url, { timeout: 8000 }, (res) => {
      let body = '';
      res.on('data', (d) => (body += d));
      res.on('end', () => resolve({ ok: res.statusCode >= 200 && res.statusCode < 300, status: res.statusCode, body }));
    });
    req.on('error', (e) => resolve({ ok: false, status: 0, body: e.message }));
    req.on('timeout', () => { req.destroy(); resolve({ ok: false, status: 0, body: 'Request timed out' }); });
    req.end();
  });
}

/**
 * POST JSON to a URL with a configurable timeout.
 *
 * @param {string} url
 * @param {object} body
 * @param {number} [timeoutMs]
 * @returns {Promise<{ ok: boolean; status: number; body: string }>}
 */
function httpPost(url, body, timeoutMs = 30_000) {
  const parsed = new URL(url);
  const bodyStr = JSON.stringify(body);
  const options = {
    hostname: parsed.hostname,
    port: parsed.port || (parsed.protocol === 'https:' ? 443 : 80),
    path: parsed.pathname + parsed.search,
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'content-length': Buffer.byteLength(bodyStr),
    },
    timeout: timeoutMs,
  };

  const mod = parsed.protocol === 'https:' ? require('https') : require('http');

  return new Promise((resolve) => {
    const req = mod.request(options, (res) => {
      let raw = '';
      res.on('data', (d) => (raw += d));
      res.on('end', () => resolve({
        ok: res.statusCode >= 200 && res.statusCode < 300,
        status: res.statusCode,
        body: raw,
      }));
    });
    req.on('timeout', () => {
      req.destroy();
      resolve({ ok: false, status: 0, body: `Request timed out after ${timeoutMs}ms` });
    });
    req.on('error', (e) => resolve({ ok: false, status: 0, body: e.message }));
    req.write(bodyStr);
    req.end();
  });
}

// ─── Logger ───────────────────────────────────────────────────────────────────

function log(msg) {
  if (VERBOSE || !isMainThread) process.stdout.write(msg + '\n');
  else if (!msg.startsWith('[SHORT]') && !msg.startsWith('[LONG]')) process.stdout.write(msg + '\n');
}
