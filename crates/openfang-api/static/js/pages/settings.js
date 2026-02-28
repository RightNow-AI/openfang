// OpenFang Settings Page — Provider Hub, Model Catalog, Config, Tools + Security, Network, Migration tabs
'use strict';

function settingsPage() {
  return {
    tab: 'providers',
    sysInfo: {},
    usageData: [],
    tools: [],
    config: {},
    providers: [],
    models: [],
    toolSearch: '',
    modelSearch: '',
    modelProviderFilter: '',
    modelTierFilter: '',
    providerKeyInputs: {},
    providerUrlInputs: {},
    providerUrlSaving: {},
    providerTesting: {},
    providerTestResults: {},
    copilotOAuth: { polling: false, userCode: '', verificationUri: '', pollId: '', interval: 5 },
    loading: true,
    loadError: '',

    // -- Dynamic config state --
    configSchema: null,
    configValues: {},
    configDirty: {},
    configSaving: {},

    // -- Security state --
    securityData: null,
    secLoading: false,
    verifyingChain: false,
    chainResult: null,

    coreFeatures: [
      {
        name: 'Path Traversal Prevention', key: 'path_traversal',
        description: 'Blocks directory escape attacks (../) in all file operations. Two-phase validation: syntactic rejection of path components, then canonicalization to normalize symlinks.',
        threat: 'Directory escape, privilege escalation via symlinks',
        impl: 'host_functions.rs — safe_resolve_path() + safe_resolve_parent()'
      },
      {
        name: 'SSRF Protection', key: 'ssrf_protection',
        description: 'Blocks outbound requests to private IPs, localhost, and cloud metadata endpoints (AWS/GCP/Azure). Validates DNS resolution results to defeat rebinding attacks.',
        threat: 'Internal network reconnaissance, cloud credential theft',
        impl: 'host_functions.rs — is_ssrf_target() + is_private_ip()'
      },
      {
        name: 'Capability-Based Access Control', key: 'capability_system',
        description: 'Deny-by-default permission system. Every agent operation (file I/O, network, shell, memory, spawn) requires an explicit capability grant in the manifest.',
        threat: 'Unauthorized resource access, sandbox escape',
        impl: 'host_functions.rs — check_capability() on every host function'
      },
      {
        name: 'Privilege Escalation Prevention', key: 'privilege_escalation_prevention',
        description: 'When a parent agent spawns a child, the kernel enforces child capabilities are a subset of parent capabilities. No agent can grant rights it does not have.',
        threat: 'Capability escalation through agent spawning chains',
        impl: 'kernel_handle.rs — spawn_agent_checked()'
      },
      {
        name: 'Subprocess Environment Isolation', key: 'subprocess_isolation',
        description: 'Child processes (shell tools) inherit only a safe allow-list of environment variables. API keys, database passwords, and secrets are never leaked to subprocesses.',
        threat: 'Secret exfiltration via child process environment',
        impl: 'subprocess_sandbox.rs — env_clear() + SAFE_ENV_VARS'
      },
      {
        name: 'Security Headers', key: 'security_headers',
        description: 'Every HTTP response includes CSP, X-Frame-Options: DENY, X-Content-Type-Options: nosniff, Referrer-Policy, and X-XSS-Protection headers.',
        threat: 'XSS, clickjacking, MIME sniffing, content injection',
        impl: 'middleware.rs — security_headers()'
      },
      {
        name: 'Wire Protocol Authentication', key: 'wire_hmac_auth',
        description: 'Agent-to-agent OFP connections use HMAC-SHA256 mutual authentication with nonce-based handshake and constant-time signature comparison (subtle crate).',
        threat: 'Man-in-the-middle attacks on mesh network',
        impl: 'peer.rs — hmac_sign() + hmac_verify()'
      },
      {
        name: 'Request ID Tracking', key: 'request_id_tracking',
        description: 'Every API request receives a unique UUID (x-request-id header) and is logged with method, path, status code, and latency for full traceability.',
        threat: 'Untraceable actions, forensic blind spots',
        impl: 'middleware.rs — request_logging()'
      }
    ],

    configurableFeatures: [
      {
        name: 'API Rate Limiting', key: 'rate_limiter',
        description: 'GCRA (Generic Cell Rate Algorithm) with cost-aware tokens. Different endpoints cost different amounts — spawning an agent costs 50 tokens, health check costs 1.',
        configHint: 'Hard-coded: 500 tokens/minute per IP. Edit rate_limiter.rs to tune.',
        valueKey: 'rate_limiter'
      },
      {
        name: 'WebSocket Connection Limits', key: 'websocket_limits',
        description: 'Per-IP connection cap prevents connection exhaustion. Idle timeout closes abandoned connections. Message rate limiting prevents flooding.',
        configHint: 'Hard-coded: 5 connections/IP, 30min idle timeout, 64KB max message. Edit ws.rs to tune.',
        valueKey: 'websocket_limits'
      },
      {
        name: 'WASM Dual Metering', key: 'wasm_sandbox',
        description: 'WASM modules run with two independent resource limits: fuel metering (CPU instruction count) and epoch interruption (wall-clock timeout with watchdog thread).',
        configHint: 'Default: 1M fuel units, 30s timeout. Configurable per-agent via SandboxConfig.',
        valueKey: 'wasm_sandbox'
      },
      {
        name: 'Bearer Token Authentication', key: 'auth',
        description: 'All non-health endpoints require Authorization: Bearer header. When no API key is configured, all requests are restricted to localhost only.',
        configHint: 'Set api_key in ~/.openfang/config.toml for remote access. Empty = localhost only.',
        valueKey: 'auth'
      }
    ],

    monitoringFeatures: [
      {
        name: 'Merkle Audit Trail', key: 'audit_trail',
        description: 'Every security-critical action is appended to an immutable, tamper-evident log. Each entry is cryptographically linked to the previous via SHA-256 hash chain.',
        configHint: 'Always active. Verify chain integrity from the Audit Log page.',
        valueKey: 'audit_trail'
      },
      {
        name: 'Information Flow Taint Tracking', key: 'taint_tracking',
        description: 'Labels data by provenance (ExternalNetwork, UserInput, PII, Secret, UntrustedAgent) and blocks unsafe flows: external data cannot reach shell_exec, secrets cannot reach network.',
        configHint: 'Always active. Prevents data flow attacks automatically.',
        valueKey: 'taint_tracking'
      },
      {
        name: 'Ed25519 Manifest Signing', key: 'manifest_signing',
        description: 'Agent manifests can be cryptographically signed with Ed25519. Verify manifest integrity before loading to prevent supply chain tampering.',
        configHint: 'Available for use. Sign manifests with ed25519-dalek for verification.',
        valueKey: 'manifest_signing'
      }
    ],

    // -- Peers state --
    peers: [],
    peersLoading: false,
    peersLoadError: '',
    _peerPollTimer: null,

    // -- Migration state --
    migStep: 'intro',
    detecting: false,
    scanning: false,
    migrating: false,
    sourcePath: '',
    targetPath: '',
    scanResult: null,
    migResult: null,

    // -- Settings load --
    async loadSettings() {
      this.loading = true;
      this.loadError = '';
      try {
        await Promise.all([
          this.loadSysInfo(),
          this.loadUsage(),
          this.loadTools(),
          this.loadConfig(),
          this.loadProviders(),
          this.loadModels()
        ]);
      } catch(e) {
        this.loadError = e.message || 'Could not load settings.';
      }
      this.loading = false;
    },

    async loadData() { return this.loadSettings(); },

    async loadSysInfo() {
      try {
        var ver = await OpenFangAPI.get('/api/version');
        var status = await OpenFangAPI.get('/api/status');
        this.sysInfo = {
          version: ver.version || '-',
          platform: ver.platform || '-',
          arch: ver.arch || '-',
          uptime_seconds: status.uptime_seconds || 0,
          agent_count: status.agent_count || 0,
          default_provider: status.default_provider || '-',
          default_model: status.default_model || '-'
        };
      } catch(e) { throw e; }
    },

    async loadUsage() {
      try {
        var data = await OpenFangAPI.get('/api/usage');
        this.usageData = data.agents || [];
      } catch(e) { this.usageData = []; }
    },

    async loadTools() {
      try {
        var data = await OpenFangAPI.get('/api/tools');
        this.tools = data.tools || [];
      } catch(e) { this.tools = []; }
    },

    async loadConfig() {
      try {
        this.config = await OpenFangAPI.get('/api/config');
      } catch(e) { this.config = {}; }
    },

    async loadProviders() {
      try {
        var data = await OpenFangAPI.get('/api/providers');
        this.providers = data.providers || [];
        for (var i = 0; i < this.providers.length; i++) {
          var p = this.providers[i];
          if (p.is_local && p.base_url && !this.providerUrlInputs[p.id]) {
            this.providerUrlInputs[p.id] = p.base_url;
          }
        }
      } catch(e) { this.providers = []; }
    },

    async loadModels() {
      try {
        var data = await OpenFangAPI.get('/api/models');
        this.models = data.models || [];
      } catch(e) { this.models = []; }
    },

    async loadConfigSchema() {
      try {
        var results = await Promise.all([
          OpenFangAPI.get('/api/config/schema').catch(function() { return {}; }),
          OpenFangAPI.get('/api/config')
        ]);
        this.configSchema = results[0].sections || null;
        this.configValues = results[1] || {};
      } catch(e) { /* silent */ }
    },

    isConfigDirty(section, field) {
      return this.configDirty[section + '.' + field] === true;
    },

    markConfigDirty(section, field) {
      this.configDirty[section + '.' + field] = true;
    },

    async saveConfigField(section, field, value) {
      var key = section + '.' + field;
      this.configSaving[key] = true;
      try {
        await OpenFangAPI.post('/api/config/set', { path: key, value: value });
        this.configDirty[key] = false;
        OpenFangToast.success('Saved ' + key);
      } catch(e) {
        OpenFangToast.error('Failed to save: ' + e.message);
      }
      this.configSaving[key] = false;
    },

    get filteredTools() {
      var q = this.toolSearch.toLowerCase().trim();
      if (!q) return this.tools;
      return this.tools.filter(function(t) {
        return t.name.toLowerCase().indexOf(q) !== -1 ||
               (t.description || '').toLowerCase().indexOf(q) !== -1;
      });
    },

    get filteredModels() {
      var self = this;
      return this.models.filter(function(m) {
        if (self.modelProviderFilter && m.provider !== self.modelProviderFilter) return false;
        if (self.modelTierFilter && m.tier !== self.modelTierFilter) return false;
        if (self.modelSearch) {
          var q = self.modelSearch.toLowerCase();
          if (m.id.toLowerCase().indexOf(q) === -1 &&
              (m.display_name || '').toLowerCase().indexOf(q) === -1) return false;
        }
        return true;
      });
    },

    get uniqueProviderNames() {
      var seen = {};
      this.models.forEach(function(m) { seen[m.provider] = true; });
      return Object.keys(seen).sort();
    },

    get uniqueTiers() {
      var seen = {};
      this.models.forEach(function(m) { if (m.tier) seen[m.tier] = true; });
      return Object.keys(seen).sort();
    },

    providerAuthClass(p) {
      if (p.auth_status === 'configured') return 'auth-configured';
      if (p.auth_status === 'not_set' || p.auth_status === 'missing') return 'auth-not-set';
      return 'auth-no-key';
    },

    providerAuthText(p) {
      if (p.auth_status === 'configured') return 'Configured';
      if (p.auth_status === 'not_set' || p.auth_status === 'missing') return 'Not Set';
      return 'No Key Needed';
    },

    providerCardClass(p) {
      if (p.auth_status === 'configured') return 'configured';
      if (p.auth_status === 'not_set' || p.auth_status === 'missing') return 'not-configured';
      return 'no-key';
    },

    tierBadgeClass(tier) {
      if (!tier) return '';
      var t = tier.toLowerCase();
      if (t === 'frontier') return 'tier-frontier';
      if (t === 'smart') return 'tier-smart';
      if (t === 'balanced') return 'tier-balanced';
      if (t === 'fast') return 'tier-fast';
      return '';
    },

    formatCost(cost) {
      if (!cost && cost !== 0) return '-';
      return '$' + cost.toFixed(4);
    },

    formatContext(ctx) {
      if (!ctx) return '-';
      if (ctx >= 1000000) return (ctx / 1000000).toFixed(1) + 'M';
      if (ctx >= 1000) return Math.round(ctx / 1000) + 'K';
      return String(ctx);
    },

    formatUptime(secs) {
      if (!secs) return '-';
      var h = Math.floor(secs / 3600);
      var m = Math.floor((secs % 3600) / 60);
      var s = secs % 60;
      if (h > 0) return h + 'h ' + m + 'm';
      if (m > 0) return m + 'm ' + s + 's';
      return s + 's';
    },

    async saveProviderKey(provider) {
      var key = this.providerKeyInputs[provider.id];
      if (!key || !key.trim()) { OpenFangToast.error('Please enter an API key'); return; }
      try {
        await OpenFangAPI.post('/api/providers/' + encodeURIComponent(provider.id) + '/key', { key: key.trim() });
        OpenFangToast.success('API key saved for ' + provider.display_name);
        this.providerKeyInputs[provider.id] = '';
        await this.loadProviders();
        await this.loadModels();
      } catch(e) {
        OpenFangToast.error('Failed to save key: ' + e.message);
      }
    },

    async removeProviderKey(provider) {
      try {
        await OpenFangAPI.del('/api/providers/' + encodeURIComponent(provider.id) + '/key');
        OpenFangToast.success('API key removed for ' + provider.display_name);
        await this.loadProviders();
        await this.loadModels();
      } catch(e) {
        OpenFangToast.error('Failed to remove key: ' + e.message);
      }
    },

    async startCopilotOAuth() {
      this.copilotOAuth.polling = true;
      this.copilotOAuth.userCode = '';
      try {
        var resp = await OpenFangAPI.post('/api/providers/github-copilot/oauth/start', {});
        this.copilotOAuth.userCode = resp.user_code;
        this.copilotOAuth.verificationUri = resp.verification_uri;
        this.copilotOAuth.pollId = resp.poll_id;
        this.copilotOAuth.interval = resp.interval || 5;
        window.open(resp.verification_uri, '_blank');
        this.pollCopilotOAuth();
      } catch(e) {
        OpenFangToast.error('Failed to start Copilot login: ' + e.message);
        this.copilotOAuth.polling = false;
      }
    },

    pollCopilotOAuth() {
      var self = this;
      setTimeout(async function() {
        if (!self.copilotOAuth.pollId) return;
        try {
          var resp = await OpenFangAPI.get('/api/providers/github-copilot/oauth/poll/' + self.copilotOAuth.pollId);
          if (resp.status === 'complete') {
            OpenFangToast.success('GitHub Copilot authenticated successfully!');
            self.copilotOAuth = { polling: false, userCode: '', verificationUri: '', pollId: '', interval: 5 };
            await self.loadProviders();
            await self.loadModels();
          } else if (resp.status === 'pending') {
            if (resp.interval) self.copilotOAuth.interval = resp.interval;
            self.pollCopilotOAuth();
          } else if (resp.status === 'expired') {
            OpenFangToast.error('Device code expired. Please try again.');
            self.copilotOAuth = { polling: false, userCode: '', verificationUri: '', pollId: '', interval: 5 };
          } else if (resp.status === 'denied') {
            OpenFangToast.error('Access denied by user.');
            self.copilotOAuth = { polling: false, userCode: '', verificationUri: '', pollId: '', interval: 5 };
          } else {
            OpenFangToast.error('OAuth error: ' + (resp.error || resp.status));
            self.copilotOAuth = { polling: false, userCode: '', verificationUri: '', pollId: '', interval: 5 };
          }
        } catch(e) {
          OpenFangToast.error('Poll error: ' + e.message);
          self.copilotOAuth = { polling: false, userCode: '', verificationUri: '', pollId: '', interval: 5 };
        }
      }, self.copilotOAuth.interval * 1000);
    },

    async testProvider(provider) {
      this.providerTesting[provider.id] = true;
      this.providerTestResults[provider.id] = null;
      try {
        var result = await OpenFangAPI.post('/api/providers/' + encodeURIComponent(provider.id) + '/test', {});
        this.providerTestResults[provider.id] = result;
        if (result.status === 'ok') {
          OpenFangToast.success(provider.display_name + ' connected (' + (result.latency_ms || '?') + 'ms)');
        } else {
          OpenFangToast.error(provider.display_name + ': ' + (result.error || 'Connection failed'));
        }
      } catch(e) {
        this.providerTestResults[provider.id] = { status: 'error', error: e.message };
        OpenFangToast.error('Test failed: ' + e.message);
      }
      this.providerTesting[provider.id] = false;
    },

    async saveProviderUrl(provider) {
      var url = this.providerUrlInputs[provider.id];
      if (!url || !url.trim()) { OpenFangToast.error('Please enter a base URL'); return; }
      url = url.trim();
      if (url.indexOf('http://') !== 0 && url.indexOf('https://') !== 0) {
        OpenFangToast.error('URL must start with http:// or https://'); return;
      }
      this.providerUrlSaving[provider.id] = true;
      try {
        var result = await OpenFangAPI.put('/api/providers/' + encodeURIComponent(provider.id) + '/url', { base_url: url });
        if (result.reachable) {
          OpenFangToast.success(provider.display_name + ' URL saved &mdash; reachable (' + (result.latency_ms || '?') + 'ms)');
        } else {
          OpenFangToast.warning(provider.display_name + ' URL saved but not reachable');
        }
        await this.loadProviders();
      } catch(e) {
        OpenFangToast.error('Failed to save URL: ' + e.message);
      }
      this.providerUrlSaving[provider.id] = false;
    },

    // -- Security methods --
    async loadSecurity() {
      this.secLoading = true;
      try {
        this.securityData = await OpenFangAPI.get('/api/security');
      } catch(e) {
        this.securityData = null;
      }
      this.secLoading = false;
    },

    isActive(key) {
      if (!this.securityData) return true;
      var core = this.securityData.core_protections || {};
      if (core[key] !== undefined) return core[key];
      return true;
    },

    getConfigValue(key) {
      if (!this.securityData) return null;
      var cfg = this.securityData.configurable || {};
      return cfg[key] || null;
    },

    getMonitoringValue(key) {
      if (!this.securityData) return null;
      var mon = this.securityData.monitoring || {};
      return mon[key] || null;
    },

    formatConfigValue(feature) {
      var val = this.getConfigValue(feature.valueKey);
      if (!val) return feature.configHint;
      switch (feature.valueKey) {
        case 'rate_limiter':
          return 'Algorithm: ' + (val.algorithm || 'GCRA') + ' | ' + (val.tokens_per_minute || 500) + ' tokens/min per IP';
        case 'websocket_limits':
          return 'Max ' + (val.max_per_ip || 5) + ' conn/IP | ' + Math.round((val.idle_timeout_secs || 1800) / 60) + 'min idle timeout | ' + Math.round((val.max_message_size || 65536) / 1024) + 'KB max msg';
        case 'wasm_sandbox':
          return 'Fuel: ' + (val.fuel_metering ? 'ON' : 'OFF') + ' | Epoch: ' + (val.epoch_interruption ? 'ON' : 'OFF') + ' | Timeout: ' + (val.default_timeout_secs || 30) + 's';
        case 'auth':
          return 'Mode: ' + (val.mode || 'unknown') + (val.api_key_set ? ' (key configured)' : ' (no key set)');
        default:
          return feature.configHint;
      }
    },

    formatMonitoringValue(feature) {
      var val = this.getMonitoringValue(feature.valueKey);
      if (!val) return feature.configHint;
      switch (feature.valueKey) {
        case 'audit_trail':
          return (val.enabled ? 'Active' : 'Disabled') + ' | ' + (val.algorithm || 'SHA-256') + ' | ' + (val.entry_count || 0) + ' entries logged';
        case 'taint_tracking':
          var labels = val.tracked_labels || [];
          return (val.enabled ? 'Active' : 'Disabled') + ' | Tracking: ' + labels.join(', ');
        case 'manifest_signing':
          return 'Algorithm: ' + (val.algorithm || 'Ed25519') + ' | ' + (val.available ? 'Available' : 'Not available');
        default:
          return feature.configHint;
      }
    },

    async verifyAuditChain() {
      this.verifyingChain = true;
      this.chainResult = null;
      try {
        var res = await OpenFangAPI.get('/api/audit/verify');
        this.chainResult = res;
      } catch(e) {
        this.chainResult = { valid: false, error: e.message };
      }
      this.verifyingChain = false;
    },

    // -- Peers methods --
    async loadPeers() {
      this.peersLoading = true;
      this.peersLoadError = '';
      try {
        var data = await OpenFangAPI.get('/api/peers');
        this.peers = (data.peers || []).map(function(p) {
          return {
            node_id: p.node_id,
            node_name: p.node_name,
            address: p.address,
            state: p.state,
            agent_count: (p.agents || []).length,
            protocol_version: p.protocol_version || 1
          };
        });
      } catch(e) {
        this.peers = [];
        this.peersLoadError = e.message || 'Could not load peers.';
      }
      this.peersLoading = false;
    },

    startPeerPolling() {
      var self = this;
      this.stopPeerPolling();
      this._peerPollTimer = setInterval(async function() {
        if (self.tab !== 'network') { self.stopPeerPolling(); return; }
        try {
          var data = await OpenFangAPI.get('/api/peers');
          self.peers = (data.peers || []).map(function(p) {
            return {
              node_id: p.node_id,
              node_name: p.node_name,
              address: p.address,
              state: p.state,
              agent_count: (p.agents || []).length,
              protocol_version: p.protocol_version || 1
            };
          });
        } catch(e) { /* silent */ }
      }, 15000);
    },

    stopPeerPolling() {
      if (this._peerPollTimer) { clearInterval(this._peerPollTimer); this._peerPollTimer = null; }
    },

    // -- Migration methods --
    async autoDetect() {
      this.detecting = true;
      try {
        var data = await OpenFangAPI.get('/api/migrate/detect');
        if (data.detected && data.scan) {
          this.sourcePath = data.path;
          this.scanResult = data.scan;
          this.migStep = 'preview';
        } else {
          this.migStep = 'not_found';
        }
      } catch(e) {
        this.migStep = 'not_found';
      }
      this.detecting = false;
    },

    async scanPath() {
      if (!this.sourcePath) return;
      this.scanning = true;
      try {
        var data = await OpenFangAPI.post('/api/migrate/scan', { path: this.sourcePath });
        if (data.error) {
          OpenFangToast.error('Scan error: ' + data.error);
          this.scanning = false;
          return;
        }
        this.scanResult = data;
        this.migStep = 'preview';
      } catch(e) {
        OpenFangToast.error('Scan failed: ' + e.message);
      }
      this.scanning = false;
    },

    async runMigration(dryRun) {
      this.migrating = true;
      try {
        var target = this.targetPath;
        if (!target) target = '';
        var data = await OpenFangAPI.post('/api/migrate', {
          source: 'openclaw',
          source_dir: this.sourcePath || (this.scanResult ? this.scanResult.path : ''),
          target_dir: target,
          dry_run: dryRun
        });
        this.migResult = data;
        this.migStep = 'result';
      } catch(e) {
        this.migResult = { status: 'failed', error: e.message };
        this.migStep = 'result';
      }
      this.migrating = false;
    },

    destroy() {
      this.stopPeerPolling();
    }
  };
}
