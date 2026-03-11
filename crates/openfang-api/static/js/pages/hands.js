// Operis Hands Page — curated autonomous capability packages
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

    // ── Setup Wizard State ──────────────────────────────────────────────
    setupWizard: null,
    setupStep: 1,
    setupLoading: false,
    setupChecking: false,
    clipboardMsg: null,
    _clipboardTimer: null,
    detectedPlatform: 'linux',
    installPlatforms: {},

    async loadData() {
      this.loading = true;
      this.loadError = '';
      try {
        var data = await OperisAPI.get('/api/hands');
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
        var data = await OperisAPI.get('/api/hands/active');
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
        var data = await OperisAPI.get('/api/hands/' + handId);
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
        var data = await OperisAPI.get('/api/hands/' + handId);
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
        // Initialize per-requirement platform selections
        this.installPlatforms = {};
        if (data.requirements) {
          for (var j = 0; j < data.requirements.length; j++) {
            this.installPlatforms[data.requirements[j].key] = this.detectedPlatform;
          }
        }
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
        var data = await OperisAPI.post('/api/hands/' + handId + '/install-deps', {});
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
        var data = await OperisAPI.post('/api/hands/' + this.setupWizard.id + '/check-deps', {});
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
        if (this.setupWizard.requirements[i].satisfied) count++;
      }
      return count;
    },

    get setupReqsTotal() {
      if (!this.setupWizard || !this.setupWizard.requirements) return 0;
      return this.setupWizard.requirements.length;
    },

    get setupAllReqsMet() {
      return this.setupReqsTotal > 0 && this.setupReqsMet === this.setupReqsTotal;
    },

    get setupHasReqs() {
      return this.setupReqsTotal > 0;
    },

    get setupHasSettings() {
      return this.setupWizard && this.setupWizard.settings && this.setupWizard.settings.length > 0;
    },

    setupNextStep() {
      if (this.setupStep === 1 && this.setupHasSettings) {
        this.setupStep = 2;
      } else if (this.setupStep === 1) {
        this.setupStep = 3;
      } else if (this.setupStep === 2) {
        this.setupStep = 3;
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
    },

    async launchHand() {
      if (!this.setupWizard) return;
      var handId = this.setupWizard.id;
      var config = {};
      for (var key in this.settingsValues) {
        config[key] = this.settingsValues[key];
      }
      this.activatingId = handId;
      try {
        var data = await OperisAPI.post('/api/hands/' + handId + '/activate', { config: config });
        this.showToast('Hand "' + handId + '" activated as ' + (data.agent_name || data.instance_id));
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
        await OperisAPI.post('/api/hands/instances/' + inst.instance_id + '/pause', {});
        inst.status = 'Paused';
      } catch(e) {
        this.showToast('Pause failed: ' + (e.message || 'unknown error'));
      }
    },

    async resumeHand(inst) {
      try {
        await OperisAPI.post('/api/hands/instances/' + inst.instance_id + '/resume', {});
        inst.status = 'Active';
      } catch(e) {
        this.showToast('Resume failed: ' + (e.message || 'unknown error'));
      }
    },

    async deactivate(inst) {
      var self = this;
      var handName = inst.agent_name || inst.hand_id;
      OperisToast.confirm('Deactivate Hand', 'Deactivate hand "' + handName + '"? This will kill its agent.', async function() {
        try {
          await OperisAPI.delete('/api/hands/instances/' + inst.instance_id);
          self.instances = self.instances.filter(function(i) { return i.instance_id !== inst.instance_id; });
          OperisToast.success('Hand deactivated.');
        } catch(e) {
          OperisToast.error('Deactivation failed: ' + (e.message || 'unknown error'));
        }
      });
    },

    async loadStats(inst) {
      try {
        var data = await OperisAPI.get('/api/hands/instances/' + inst.instance_id + '/stats');
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
        var data = await OperisAPI.get('/api/hands/instances/' + id + '/browser');
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
    }
  };
}
