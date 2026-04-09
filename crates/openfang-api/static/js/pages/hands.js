// OpenFang Hands Page — curated autonomous capability packages
'use strict';

function handsPage() {
  return {
    tab: 'available',
    hands: [],
    instances: [],
    loading: true,
    activeLoading: false,
    loadError: '',
    activatingId: null,
    activateResult: null,
    detailHand: null,
    settingsValues: {},
    _toastTimer: null,
    browserViewer: null,
    browserViewerOpen: false,
    _browserPollTimer: null,

    // ── Trader Dashboard State ────────────────────────────────────────────
    dashboardOpen: false,
    dashboardLoading: false,
    dashboardData: null,
    _dashboardInst: null,
    _chartEquity: null,
    _chartPnl: null,
    _chartRadar: null,

    // ── Setup Wizard State ──────────────────────────────────────────────
    setupWizard: null,
    setupStep: 1,
    setupLoading: false,
    setupChecking: false,
    clipboardMsg: null,
    _clipboardTimer: null,
    detectedPlatform: 'linux',
    installPlatforms: {},
    apiKeyInputs: {},

    async loadData() {
      this.loading = true;
      this.loadError = '';
      try {
        var data = await OpenFangAPI.get('/api/hands');
        this.hands = data.hands || [];
      } catch(e) {
        this.hands = [];
        this.loadError = e.message || 'Could not load hands.';
      }
      this.loading = false;
    },

    async loadActive() {
      this.activeLoading = true;
      try {
        var data = await OpenFangAPI.get('/api/hands/active');
        this.instances = (data.instances || []).map(function(i) {
          i._stats = null;
          return i;
        });
      } catch(e) {
        this.instances = [];
      }
      this.activeLoading = false;
    },

    getHandIcon(handId) {
      for (var i = 0; i < this.hands.length; i++) {
        if (this.hands[i].id === handId) return this.hands[i].icon;
      }
      return '\u{1F91A}';
    },

    async showDetail(handId) {
      try {
        var data = await OpenFangAPI.get('/api/hands/' + handId);
        this.detailHand = data;
      } catch(e) {
        for (var i = 0; i < this.hands.length; i++) {
          if (this.hands[i].id === handId) {
            this.detailHand = this.hands[i];
            break;
          }
        }
      }
    },

    // ── Setup Wizard ────────────────────────────────────────────────────

    async activate(handId) {
      this.openSetupWizard(handId);
    },

    async openSetupWizard(handId) {
      this.setupLoading = true;
      this.setupWizard = null;
      try {
        var data = await OpenFangAPI.get('/api/hands/' + handId);
        // Pre-populate settings defaults
        this.settingsValues = {};
        if (data.settings && data.settings.length > 0) {
          for (var i = 0; i < data.settings.length; i++) {
            var s = data.settings[i];
            this.settingsValues[s.key] = s.default || '';
          }
        }
        // Detect platform from server response, fallback to client-side
        if (data.server_platform) {
          this.detectedPlatform = data.server_platform;
        } else {
          this._detectClientPlatform();
        }
        // Initialize per-requirement platform selections and API key inputs
        this.installPlatforms = {};
        this.apiKeyInputs = {};
        if (data.requirements) {
          for (var j = 0; j < data.requirements.length; j++) {
            this.installPlatforms[data.requirements[j].key] = this.detectedPlatform;
            if (data.requirements[j].type === 'ApiKey') {
              this.apiKeyInputs[data.requirements[j].key] = '';
            }
          }
        }
        // Initialize optional instance name (for multi-instance hands).
        data.instanceName = '';
        this.setupWizard = data;
        // Skip deps step if no requirements
        var hasReqs = data.requirements && data.requirements.length > 0;
        this.setupStep = hasReqs ? 1 : 2;
      } catch(e) {
        this.showToast('Could not load hand details: ' + (e.message || 'unknown error'));
      }
      this.setupLoading = false;
    },

    _detectClientPlatform() {
      var ua = (navigator.userAgent || '').toLowerCase();
      if (ua.indexOf('mac') !== -1) {
        this.detectedPlatform = 'macos';
      } else if (ua.indexOf('win') !== -1) {
        this.detectedPlatform = 'windows';
      } else {
        this.detectedPlatform = 'linux';
      }
    },

    // ── Auto-Install Dependencies ───────────────────────────────────
    installProgress: null,   // null = idle, object = { status, current, total, results, error }

    async installDeps() {
      if (!this.setupWizard) return;
      var handId = this.setupWizard.id;
      var missing = (this.setupWizard.requirements || []).filter(function(r) { return !r.satisfied; });
      if (missing.length === 0) {
        this.showToast('All dependencies already installed!');
        return;
      }

      this.installProgress = {
        status: 'installing',
        current: 0,
        total: missing.length,
        currentLabel: missing[0] ? missing[0].label : '',
        results: [],
        error: null
      };

      try {
        var data = await OpenFangAPI.post('/api/hands/' + handId + '/install-deps', {});
        var results = data.results || [];
        this.installProgress.results = results;
        this.installProgress.current = results.length;
        this.installProgress.status = 'done';

        // Update requirements from server response
        if (data.requirements && this.setupWizard.requirements) {
          for (var i = 0; i < this.setupWizard.requirements.length; i++) {
            var existing = this.setupWizard.requirements[i];
            for (var j = 0; j < data.requirements.length; j++) {
              if (data.requirements[j].key === existing.key) {
                existing.satisfied = data.requirements[j].satisfied;
                break;
              }
            }
          }
          this.setupWizard.requirements_met = data.requirements_met;
        }

        var installed = results.filter(function(r) { return r.status === 'installed' || r.status === 'already_installed'; }).length;
        var failed = results.filter(function(r) { return r.status === 'error' || r.status === 'timeout'; }).length;

        if (data.requirements_met) {
          this.showToast('All dependencies installed successfully!');
          // Auto-advance to step 2 after a short delay
          var self = this;
          setTimeout(function() {
            self.installProgress = null;
            self.setupNextStep();
          }, 1500);
        } else if (failed > 0) {
          this.installProgress.error = failed + ' dependency(ies) failed to install. Check the details below.';
        }
      } catch(e) {
        this.installProgress = {
          status: 'error',
          current: 0,
          total: missing.length,
          currentLabel: '',
          results: [],
          error: e.message || 'Installation request failed'
        };
      }
    },

    getInstallResultIcon(status) {
      if (status === 'installed' || status === 'already_installed') return '\u2713';
      if (status === 'error' || status === 'timeout') return '\u2717';
      return '\u2022';
    },

    getInstallResultClass(status) {
      if (status === 'installed' || status === 'already_installed') return 'dep-met';
      if (status === 'error' || status === 'timeout') return 'dep-missing';
      return '';
    },

    async recheckDeps() {
      if (!this.setupWizard) return;
      this.setupChecking = true;
      try {
        var data = await OpenFangAPI.post('/api/hands/' + this.setupWizard.id + '/check-deps', {});
        if (data.requirements && this.setupWizard.requirements) {
          for (var i = 0; i < this.setupWizard.requirements.length; i++) {
            var existing = this.setupWizard.requirements[i];
            for (var j = 0; j < data.requirements.length; j++) {
              if (data.requirements[j].key === existing.key) {
                existing.satisfied = data.requirements[j].satisfied;
                break;
              }
            }
          }
          this.setupWizard.requirements_met = data.requirements_met;
        }
        if (data.requirements_met) {
          this.showToast('All dependencies satisfied!');
        }
      } catch(e) {
        this.showToast('Check failed: ' + (e.message || 'unknown'));
      }
      this.setupChecking = false;
    },

    getInstallCmd(req) {
      if (!req || !req.install) return null;
      var inst = req.install;
      var plat = this.installPlatforms[req.key] || this.detectedPlatform;
      if (plat === 'macos' && inst.macos) return inst.macos;
      if (plat === 'windows' && inst.windows) return inst.windows;
      if (plat === 'linux') {
        return inst.linux_apt || inst.linux_dnf || inst.linux_pacman || inst.pip || null;
      }
      return inst.pip || inst.macos || inst.windows || inst.linux_apt || null;
    },

    getLinuxVariant(req) {
      if (!req || !req.install) return null;
      var inst = req.install;
      var plat = this.installPlatforms[req.key] || this.detectedPlatform;
      if (plat !== 'linux') return null;
      // Return all available Linux variants
      var variants = [];
      if (inst.linux_apt) variants.push({ label: 'apt', cmd: inst.linux_apt });
      if (inst.linux_dnf) variants.push({ label: 'dnf', cmd: inst.linux_dnf });
      if (inst.linux_pacman) variants.push({ label: 'pacman', cmd: inst.linux_pacman });
      if (inst.pip) variants.push({ label: 'pip', cmd: inst.pip });
      return variants.length > 1 ? variants : null;
    },

    copyToClipboard(text) {
      var self = this;
      navigator.clipboard.writeText(text).then(function() {
        self.clipboardMsg = text;
        if (self._clipboardTimer) clearTimeout(self._clipboardTimer);
        self._clipboardTimer = setTimeout(function() { self.clipboardMsg = null; }, 2000);
      });
    },

    get setupReqsMet() {
      if (!this.setupWizard || !this.setupWizard.requirements) return 0;
      var count = 0;
      for (var i = 0; i < this.setupWizard.requirements.length; i++) {
        var req = this.setupWizard.requirements[i];
        if (req.satisfied) { count++; continue; }
        // Count API key reqs as met if user entered a value
        if (req.type === 'ApiKey' && this.apiKeyInputs[req.key] && this.apiKeyInputs[req.key].trim() !== '') count++;
      }
      return count;
    },

    get setupReqsTotal() {
      if (!this.setupWizard || !this.setupWizard.requirements) return 0;
      return this.setupWizard.requirements.length;
    },

    get setupAllReqsMet() {
      if (!this.setupWizard || !this.setupWizard.requirements) return false;
      if (this.setupReqsTotal === 0) return false;
      for (var i = 0; i < this.setupWizard.requirements.length; i++) {
        var req = this.setupWizard.requirements[i];
        if (req.satisfied) continue;
        // API key reqs are satisfied if the user entered a value in the input
        if (req.type === 'ApiKey' && this.apiKeyInputs[req.key] && this.apiKeyInputs[req.key].trim() !== '') continue;
        return false;
      }
      return true;
    },

    getSettingKeyForReq(req) {
      // Find the matching setting key for an API key requirement.
      // Convention: setting key is the lowercase version of the requirement key.
      if (!this.setupWizard || !this.setupWizard.settings) return null;
      var lowerKey = req.key.toLowerCase();
      for (var i = 0; i < this.setupWizard.settings.length; i++) {
        if (this.setupWizard.settings[i].key === lowerKey) return lowerKey;
      }
      // Fallback: try matching by check_value lowercased
      if (req.check_value) {
        var lowerCheck = req.check_value.toLowerCase();
        for (var j = 0; j < this.setupWizard.settings.length; j++) {
          if (this.setupWizard.settings[j].key === lowerCheck) return lowerCheck;
        }
      }
      return null;
    },

    get setupHasReqs() {
      return this.setupReqsTotal > 0;
    },

    get setupHasSettings() {
      return this.setupWizard && this.setupWizard.settings && this.setupWizard.settings.length > 0;
    },

    setupNextStep() {
      // When leaving step 1, sync API key inputs into settings values
      if (this.setupStep === 1) {
        this._syncApiKeysToSettings();
      }
      if (this.setupStep === 1 && this.setupHasSettings) {
        this.setupStep = 2;
      } else if (this.setupStep === 1) {
        this.setupStep = 3;
      } else if (this.setupStep === 2) {
        this.setupStep = 3;
      }
    },

    _syncApiKeysToSettings() {
      if (!this.setupWizard || !this.setupWizard.requirements) return;
      for (var i = 0; i < this.setupWizard.requirements.length; i++) {
        var req = this.setupWizard.requirements[i];
        if (req.type === 'ApiKey' && this.apiKeyInputs[req.key] && this.apiKeyInputs[req.key].trim() !== '') {
          var settingKey = this.getSettingKeyForReq(req);
          if (settingKey) {
            this.settingsValues[settingKey] = this.apiKeyInputs[req.key].trim();
          }
        }
      }
    },

    setupPrevStep() {
      if (this.setupStep === 3 && this.setupHasSettings) {
        this.setupStep = 2;
      } else if (this.setupStep === 3) {
        this.setupStep = this.setupHasReqs ? 1 : 2;
      } else if (this.setupStep === 2 && this.setupHasReqs) {
        this.setupStep = 1;
      }
    },

    closeSetupWizard() {
      this.setupWizard = null;
      this.setupStep = 1;
      this.setupLoading = false;
      this.setupChecking = false;
      this.clipboardMsg = null;
      this.installPlatforms = {};
      this.apiKeyInputs = {};
    },

    async launchHand() {
      if (!this.setupWizard) return;
      var handId = this.setupWizard.id;
      // Sync API key inputs from step 1 into settings values
      if (this.setupWizard.requirements) {
        for (var i = 0; i < this.setupWizard.requirements.length; i++) {
          var req = this.setupWizard.requirements[i];
          if (req.type === 'ApiKey' && this.apiKeyInputs[req.key] && this.apiKeyInputs[req.key].trim() !== '') {
            var settingKey = this.getSettingKeyForReq(req);
            if (settingKey) {
              this.settingsValues[settingKey] = this.apiKeyInputs[req.key].trim();
            }
          }
        }
      }
      var config = {};
      for (var key in this.settingsValues) {
        config[key] = this.settingsValues[key];
      }
      this.activatingId = handId;
      try {
        var payload = { config: config };
        var name = (this.setupWizard.instanceName || '').trim();
        if (name) {
          payload.instance_name = name;
        }
        var data = await OpenFangAPI.post('/api/hands/' + handId + '/activate', payload);
        var label = data.instance_name || data.agent_name || data.instance_id;
        this.showToast('Hand "' + handId + '" activated as ' + label);
        this.closeSetupWizard();
        await this.loadActive();
        this.tab = 'active';
      } catch(e) {
        this.showToast('Activation failed: ' + (e.message || 'unknown error'));
      }
      this.activatingId = null;
    },

    selectOption(settingKey, value) {
      this.settingsValues[settingKey] = value;
    },

    getSettingDisplayValue(setting) {
      var val = this.settingsValues[setting.key] || setting.default || '';
      if (setting.setting_type === 'toggle') {
        return val === 'true' ? 'Enabled' : 'Disabled';
      }
      if (setting.setting_type === 'select' && setting.options) {
        for (var i = 0; i < setting.options.length; i++) {
          if (setting.options[i].value === val) return setting.options[i].label;
        }
      }
      return val || '-';
    },

    // ── Existing methods ────────────────────────────────────────────────

    async pauseHand(inst) {
      try {
        await OpenFangAPI.post('/api/hands/instances/' + inst.instance_id + '/pause', {});
        inst.status = 'Paused';
      } catch(e) {
        this.showToast('Pause failed: ' + (e.message || 'unknown error'));
      }
    },

    async resumeHand(inst) {
      try {
        await OpenFangAPI.post('/api/hands/instances/' + inst.instance_id + '/resume', {});
        inst.status = 'Active';
      } catch(e) {
        this.showToast('Resume failed: ' + (e.message || 'unknown error'));
      }
    },

    async deactivate(inst) {
      var self = this;
      var handName = inst.agent_name || inst.hand_id;
      OpenFangToast.confirm('Deactivate Hand', 'Deactivate hand "' + handName + '"? This will kill its agent.', async function() {
        try {
          await OpenFangAPI.delete('/api/hands/instances/' + inst.instance_id);
          self.instances = self.instances.filter(function(i) { return i.instance_id !== inst.instance_id; });
          OpenFangToast.success('Hand deactivated.');
        } catch(e) {
          OpenFangToast.error('Deactivation failed: ' + (e.message || 'unknown error'));
        }
      });
    },

    async loadStats(inst) {
      try {
        var data = await OpenFangAPI.get('/api/hands/instances/' + inst.instance_id + '/stats');
        inst._stats = data.metrics || {};
      } catch(e) {
        inst._stats = { 'Error': { value: e.message || 'Could not load stats', format: 'text' } };
      }
    },

    formatMetric(m) {
      if (!m || m.value === null || m.value === undefined) return '-';
      if (m.format === 'duration') {
        var secs = parseInt(m.value, 10);
        if (isNaN(secs)) return String(m.value);
        var h = Math.floor(secs / 3600);
        var min = Math.floor((secs % 3600) / 60);
        var s = secs % 60;
        if (h > 0) return h + 'h ' + min + 'm';
        if (min > 0) return min + 'm ' + s + 's';
        return s + 's';
      }
      if (m.format === 'number') {
        var n = parseFloat(m.value);
        if (isNaN(n)) return String(m.value);
        return n.toLocaleString();
      }
      return String(m.value);
    },

    showToast(msg) {
      var self = this;
      this.activateResult = msg;
      if (this._toastTimer) clearTimeout(this._toastTimer);
      this._toastTimer = setTimeout(function() { self.activateResult = null; }, 4000);
    },

    // ── Browser Viewer ───────────────────────────────────────────────────

    isBrowserHand(inst) {
      return inst.hand_id === 'browser';
    },

    async openBrowserViewer(inst) {
      this.browserViewer = {
        instance_id: inst.instance_id,
        hand_id: inst.hand_id,
        agent_name: inst.agent_name,
        url: '',
        title: '',
        screenshot: '',
        content: '',
        loading: true,
        error: ''
      };
      this.browserViewerOpen = true;
      await this.refreshBrowserView();
      this.startBrowserPolling();
    },

    async refreshBrowserView() {
      if (!this.browserViewer) return;
      var id = this.browserViewer.instance_id;
      try {
        var data = await OpenFangAPI.get('/api/hands/instances/' + id + '/browser');
        if (data.active) {
          this.browserViewer.url = data.url || '';
          this.browserViewer.title = data.title || '';
          this.browserViewer.screenshot = data.screenshot_base64 || '';
          this.browserViewer.content = data.content || '';
          this.browserViewer.error = '';
        } else {
          this.browserViewer.error = 'No active browser session';
          this.browserViewer.screenshot = '';
        }
      } catch(e) {
        this.browserViewer.error = e.message || 'Could not load browser state';
      }
      this.browserViewer.loading = false;
    },

    startBrowserPolling() {
      var self = this;
      this.stopBrowserPolling();
      this._browserPollTimer = setInterval(function() {
        if (self.browserViewerOpen) {
          self.refreshBrowserView();
        } else {
          self.stopBrowserPolling();
        }
      }, 3000);
    },

    stopBrowserPolling() {
      if (this._browserPollTimer) {
        clearInterval(this._browserPollTimer);
        this._browserPollTimer = null;
      }
    },

    closeBrowserViewer() {
      this.stopBrowserPolling();
      this.browserViewerOpen = false;
      this.browserViewer = null;
    },

    // ── Trader Dashboard ──────────────────────────────────────────────────

    isTraderHand(inst) {
      return inst.hand_id === 'trader';
    },

    async openDashboard(inst) {
      this._dashboardInst = inst;
      this.dashboardOpen = true;
      this.dashboardLoading = true;
      this.dashboardData = null;
      await this._fetchDashboardData(inst);
      this.dashboardLoading = false;
      // Render charts after DOM update
      var self = this;
      setTimeout(function() { self._renderCharts(); }, 60);
    },

    async refreshDashboard() {
      if (!this._dashboardInst) return;
      this.dashboardLoading = true;
      await this._fetchDashboardData(this._dashboardInst);
      this.dashboardLoading = false;
      var self = this;
      setTimeout(function() { self._renderCharts(); }, 60);
    },

    closeDashboard() {
      this.dashboardOpen = false;
      this._destroyCharts();
      this.dashboardData = null;
      this._dashboardInst = null;
    },

    async _fetchDashboardData(inst) {
      var data = {
        agent_name: inst.agent_name || inst.hand_id,
        portfolio_value: null,
        total_pnl: null,
        win_rate: null,
        sharpe_ratio: null,
        max_drawdown: null,
        trades_count: null,
        equity_curve: [],
        daily_pnl: [],
        watchlist_heatmap: [],
        signal_radar: null,
        recent_trades: []
      };

      // Fetch basic stats from the hand stats endpoint
      try {
        var stats = await OpenFangAPI.get('/api/hands/instances/' + inst.instance_id + '/stats');
        var m = stats.metrics || {};
        if (m['Portfolio Value']) data.portfolio_value = this._metricVal(m['Portfolio Value']);
        if (m['Total P&L']) data.total_pnl = this._metricVal(m['Total P&L']);
        if (m['Win Rate']) data.win_rate = this._metricVal(m['Win Rate']);
        if (m['Sharpe Ratio']) data.sharpe_ratio = this._metricVal(m['Sharpe Ratio']);
        if (m['Max Drawdown']) data.max_drawdown = this._metricVal(m['Max Drawdown']);
        if (m['Trades Executed']) data.trades_count = this._metricVal(m['Trades Executed']);
      } catch(e) {
        // Stats endpoint might fail — continue with KV data
      }

      // Fetch rich chart data from agent memory KV
      var agentId = inst.agent_id || 'shared';
      var kvKeys = [
        'trader_hand_equity_curve',
        'trader_hand_daily_pnl',
        'trader_hand_watchlist_heatmap',
        'trader_hand_signal_radar',
        'trader_hand_recent_trades',
        'trader_hand_portfolio_value',
        'trader_hand_total_pnl',
        'trader_hand_win_rate',
        'trader_hand_sharpe_ratio',
        'trader_hand_max_drawdown',
        'trader_hand_trades_count'
      ];

      for (var i = 0; i < kvKeys.length; i++) {
        try {
          var resp = await OpenFangAPI.get('/api/memory/agents/' + agentId + '/kv/' + kvKeys[i]);
          if (resp && resp.value !== null && resp.value !== undefined) {
            var val = resp.value;
            this._applyKvToData(data, kvKeys[i], val);
          }
        } catch(e) {
          // Key might not exist yet — that's fine
        }
      }

      this.dashboardData = data;
    },

    _metricVal(metric) {
      if (!metric) return null;
      var v = metric.value;
      if (v === null || v === undefined) return null;
      // Values come as JSON values — could be string, number, etc.
      if (typeof v === 'string') return v;
      return String(v);
    },

    _applyKvToData(data, key, val) {
      // Values from KV can be strings (JSON-encoded) or already parsed
      var parsed = val;
      if (typeof val === 'string') {
        try { parsed = JSON.parse(val); } catch(e) { parsed = val; }
      }

      switch(key) {
        case 'trader_hand_portfolio_value':
          if (!data.portfolio_value) data.portfolio_value = String(parsed);
          break;
        case 'trader_hand_total_pnl':
          if (!data.total_pnl) data.total_pnl = String(parsed);
          break;
        case 'trader_hand_win_rate':
          if (!data.win_rate) data.win_rate = String(parsed);
          break;
        case 'trader_hand_sharpe_ratio':
          if (!data.sharpe_ratio) data.sharpe_ratio = String(parsed);
          break;
        case 'trader_hand_max_drawdown':
          if (!data.max_drawdown) data.max_drawdown = String(parsed);
          break;
        case 'trader_hand_trades_count':
          if (!data.trades_count) data.trades_count = String(parsed);
          break;
        case 'trader_hand_equity_curve':
          if (Array.isArray(parsed)) data.equity_curve = parsed;
          break;
        case 'trader_hand_daily_pnl':
          if (Array.isArray(parsed)) data.daily_pnl = parsed;
          break;
        case 'trader_hand_watchlist_heatmap':
          if (Array.isArray(parsed)) data.watchlist_heatmap = parsed;
          break;
        case 'trader_hand_signal_radar':
          if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) data.signal_radar = parsed;
          break;
        case 'trader_hand_recent_trades':
          if (Array.isArray(parsed)) data.recent_trades = parsed;
          break;
      }
    },

    _destroyCharts() {
      if (this._chartEquity) { this._chartEquity.destroy(); this._chartEquity = null; }
      if (this._chartPnl) { this._chartPnl.destroy(); this._chartPnl = null; }
      if (this._chartRadar) { this._chartRadar.destroy(); this._chartRadar = null; }
    },

    _renderCharts() {
      if (typeof Chart === 'undefined') return;
      this._destroyCharts();
      if (!this.dashboardData) return;

      var d = this.dashboardData;

      // Detect theme
      var isDark = document.documentElement.getAttribute('data-theme') === 'dark' ||
        (!document.documentElement.getAttribute('data-theme') && window.matchMedia('(prefers-color-scheme: dark)').matches);
      var gridColor = isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.08)';
      var textColor = isDark ? '#8A8380' : '#6B6560';
      var accentColor = '#FF5C00';
      var successColor = isDark ? '#4ADE80' : '#22C55E';
      var errorColor = '#EF4444';

      // ── Equity Curve ──
      if (d.equity_curve && d.equity_curve.length > 0) {
        var eqCanvas = document.getElementById('traderEquityChart');
        if (eqCanvas) {
          var labels = [];
          var values = [];
          for (var i = 0; i < d.equity_curve.length; i++) {
            labels.push(d.equity_curve[i].date || '');
            values.push(parseFloat(d.equity_curve[i].value) || 0);
          }
          // Determine gradient
          var eqCtx = eqCanvas.getContext('2d');
          var gradient = eqCtx.createLinearGradient(0, 0, 0, eqCanvas.parentElement.clientHeight || 180);
          gradient.addColorStop(0, isDark ? 'rgba(255, 92, 0, 0.25)' : 'rgba(255, 92, 0, 0.15)');
          gradient.addColorStop(1, 'rgba(255, 92, 0, 0)');

          this._chartEquity = new Chart(eqCtx, {
            type: 'line',
            data: {
              labels: labels,
              datasets: [{
                data: values,
                borderColor: accentColor,
                backgroundColor: gradient,
                borderWidth: 2,
                fill: true,
                tension: 0.3,
                pointRadius: d.equity_curve.length > 20 ? 0 : 3,
                pointHoverRadius: 5,
                pointBackgroundColor: accentColor
              }]
            },
            options: {
              responsive: true,
              maintainAspectRatio: false,
              interaction: { mode: 'index', intersect: false },
              plugins: {
                legend: { display: false },
                tooltip: {
                  backgroundColor: isDark ? '#1a1a1a' : '#fff',
                  titleColor: textColor,
                  bodyColor: isDark ? '#e0e0e0' : '#333',
                  borderColor: gridColor,
                  borderWidth: 1,
                  padding: 10,
                  callbacks: {
                    label: function(ctx) {
                      return '$' + ctx.parsed.y.toLocaleString(undefined, {minimumFractionDigits: 2, maximumFractionDigits: 2});
                    }
                  }
                }
              },
              scales: {
                x: {
                  grid: { color: gridColor },
                  ticks: { color: textColor, maxTicksLimit: 8, font: { size: 10 } }
                },
                y: {
                  grid: { color: gridColor },
                  ticks: {
                    color: textColor,
                    font: { size: 10 },
                    callback: function(v) { return '$' + v.toLocaleString(); }
                  }
                }
              }
            }
          });
        }
      }

      // ── Daily P&L Bar Chart ──
      if (d.daily_pnl && d.daily_pnl.length > 0) {
        var pnlCanvas = document.getElementById('traderPnlChart');
        if (pnlCanvas) {
          var pnlLabels = [];
          var pnlValues = [];
          var pnlColors = [];
          for (var j = 0; j < d.daily_pnl.length; j++) {
            pnlLabels.push(d.daily_pnl[j].date || '');
            var pnlVal = parseFloat(d.daily_pnl[j].pnl) || 0;
            pnlValues.push(pnlVal);
            pnlColors.push(pnlVal >= 0 ? successColor : errorColor);
          }

          this._chartPnl = new Chart(pnlCanvas.getContext('2d'), {
            type: 'bar',
            data: {
              labels: pnlLabels,
              datasets: [{
                data: pnlValues,
                backgroundColor: pnlColors,
                borderRadius: 3,
                borderSkipped: false
              }]
            },
            options: {
              responsive: true,
              maintainAspectRatio: false,
              plugins: {
                legend: { display: false },
                tooltip: {
                  backgroundColor: isDark ? '#1a1a1a' : '#fff',
                  titleColor: textColor,
                  bodyColor: isDark ? '#e0e0e0' : '#333',
                  borderColor: gridColor,
                  borderWidth: 1,
                  padding: 10,
                  callbacks: {
                    label: function(ctx) {
                      var v = ctx.parsed.y;
                      return (v >= 0 ? '+$' : '-$') + Math.abs(v).toLocaleString(undefined, {minimumFractionDigits: 2, maximumFractionDigits: 2});
                    }
                  }
                }
              },
              scales: {
                x: {
                  grid: { display: false },
                  ticks: { color: textColor, maxTicksLimit: 7, font: { size: 10 } }
                },
                y: {
                  grid: { color: gridColor },
                  ticks: {
                    color: textColor,
                    font: { size: 10 },
                    callback: function(v) {
                      return (v >= 0 ? '+$' : '-$') + Math.abs(v).toLocaleString();
                    }
                  }
                }
              }
            }
          });
        }
      }

      // ── Signal Radar Chart ──
      if (d.signal_radar) {
        var radarCanvas = document.getElementById('traderRadarChart');
        if (radarCanvas) {
          var radarLabels = [];
          var radarValues = [];
          var keys = ['technical', 'fundamental', 'sentiment', 'macro'];
          var displayLabels = ['Technical', 'Fundamental', 'Sentiment', 'Macro'];
          for (var k = 0; k < keys.length; k++) {
            radarLabels.push(displayLabels[k]);
            radarValues.push(parseFloat(d.signal_radar[keys[k]]) || 0);
          }

          this._chartRadar = new Chart(radarCanvas.getContext('2d'), {
            type: 'radar',
            data: {
              labels: radarLabels,
              datasets: [{
                data: radarValues,
                borderColor: accentColor,
                backgroundColor: isDark ? 'rgba(255, 92, 0, 0.2)' : 'rgba(255, 92, 0, 0.12)',
                borderWidth: 2,
                pointBackgroundColor: accentColor,
                pointRadius: 4,
                pointHoverRadius: 6
              }]
            },
            options: {
              responsive: true,
              maintainAspectRatio: true,
              plugins: {
                legend: { display: false },
                tooltip: {
                  backgroundColor: isDark ? '#1a1a1a' : '#fff',
                  titleColor: textColor,
                  bodyColor: isDark ? '#e0e0e0' : '#333',
                  borderColor: gridColor,
                  borderWidth: 1,
                  padding: 10,
                  callbacks: {
                    label: function(ctx) { return ctx.parsed.r + '/100'; }
                  }
                }
              },
              scales: {
                r: {
                  min: 0,
                  max: 100,
                  beginAtZero: true,
                  grid: { color: gridColor },
                  angleLines: { color: gridColor },
                  pointLabels: {
                    color: textColor,
                    font: { size: 11, weight: '600' }
                  },
                  ticks: {
                    color: textColor,
                    backdropColor: 'transparent',
                    stepSize: 25,
                    font: { size: 9 }
                  }
                }
              }
            }
          });
        }
      }
    }
  };
}
