// OpenFang Channels Page — OpenClaw-style setup UX with QR code support
'use strict';

function channelsPage() {
  return {
    allChannels: [],
    categoryFilter: 'all',
    searchQuery: '',
    setupModal: null,
    configuring: false,
    testing: {},
    formValues: {},
    showAdvanced: false,
    showBusinessApi: false,
    loading: true,
    loadError: '',
    pollTimer: null,

    // Setup flow step tracking
    setupStep: 1, // 1=Configure, 2=Verify, 3=Ready
    testPassed: false,

    // WhatsApp QR state
    qr: {
      loading: false,
      available: false,
      dataUrl: '',
      sessionId: '',
      message: '',
      help: '',
      connected: false,
      expired: false,
      error: ''
    },
    qrPollTimer: null,

    categories: [
      { key: 'all', label: 'All' },
      { key: 'messaging', label: 'Messaging' },
      { key: 'social', label: 'Social' },
      { key: 'enterprise', label: 'Enterprise' },
      { key: 'developer', label: 'Developer' },
      { key: 'notifications', label: 'Notifications' }
    ],

    get filteredChannels() {
      var self = this;
      return this.allChannels.filter(function(ch) {
        if (self.categoryFilter !== 'all' && ch.category !== self.categoryFilter) return false;
        if (self.searchQuery) {
          var q = self.searchQuery.toLowerCase();
          return ch.name.toLowerCase().indexOf(q) !== -1 ||
                 ch.display_name.toLowerCase().indexOf(q) !== -1 ||
                 ch.description.toLowerCase().indexOf(q) !== -1;
        }
        return true;
      });
    },

    get configuredCount() {
      return this.allChannels.filter(function(ch) { return ch.configured; }).length;
    },

    categoryCount(cat) {
      var all = this.allChannels.filter(function(ch) { return cat === 'all' || ch.category === cat; });
      var configured = all.filter(function(ch) { return ch.configured; });
      return configured.length + '/' + all.length;
    },

    isDingTalkChannel() {
      return this.setupModal && this.setupModal.name === 'dingtalk';
    },

    dingTalkMode() {
      if (!this.isDingTalkChannel()) return '';
      return this.formValues.mode === 'stream' ? 'stream' : 'webhook';
    },

    visibleFields() {
      if (!this.setupModal || !this.setupModal.fields) return [];
      var fields = this.setupModal.fields;
      if (!this.isDingTalkChannel()) return fields;
      var mode = this.dingTalkMode();
      return fields.filter(function(f) {
        if (f.key === 'mode' || f.key === 'default_agent') return true;
        if (mode === 'stream') {
          return f.key === 'client_id_env' || f.key === 'client_secret_env';
        }
        return f.key === 'access_token_env' || f.key === 'secret_env' || f.key === 'webhook_port';
      });
    },

    basicFields() {
      return this.visibleFields().filter(function(f) { return !f.advanced; });
    },

    advancedFields() {
      return this.visibleFields().filter(function(f) { return f.advanced; });
    },

    hasAdvanced() {
      return this.advancedFields().length > 0;
    },

    isQrChannel() {
      return this.setupModal && this.setupModal.setup_type === 'qr';
    },

    async loadChannels() {
      this.loading = true;
      this.loadError = '';
      try {
        var data = await OpenFangAPI.get('/api/channels');
        this.allChannels = (data.channels || []).map(function(ch) {
          ch.connected = ch.configured && ch.has_token;
          return ch;
        });
      } catch(e) {
        this.loadError = e.message || 'Could not load channels.';
      }
      this.loading = false;
      this.startPolling();
    },

    async loadData() { return this.loadChannels(); },

    startPolling() {
      var self = this;
      if (this.pollTimer) clearInterval(this.pollTimer);
      this.pollTimer = setInterval(function() { self.refreshStatus(); }, 15000);
    },

    async refreshStatus() {
      try {
        var data = await OpenFangAPI.get('/api/channels');
        var byName = {};
        (data.channels || []).forEach(function(ch) { byName[ch.name] = ch; });
        this.allChannels.forEach(function(c) {
          var fresh = byName[c.name];
          if (fresh) {
            c.configured = fresh.configured;
            c.has_token = fresh.has_token;
            c.connected = fresh.configured && fresh.has_token;
            c.fields = fresh.fields;
          }
        });
      } catch(e) { console.warn('Channel refresh failed:', e.message); }
    },

    statusBadge(ch) {
      if (!ch.configured) return { text: 'Not Configured', cls: 'badge-muted' };
      if (!ch.has_token) return { text: 'Missing Token', cls: 'badge-warn' };
      if (ch.connected) return { text: 'Ready', cls: 'badge-success' };
      return { text: 'Configured', cls: 'badge-info' };
    },

    difficultyClass(d) {
      if (d === 'Easy') return 'difficulty-easy';
      if (d === 'Hard') return 'difficulty-hard';
      return 'difficulty-medium';
    },

    openSetup(ch) {
      this.setupModal = ch;
      // Pre-populate form values from saved config (non-secret fields).
      var vals = {};
      if (ch.fields) {
        ch.fields.forEach(function(f) {
          if (f.value !== undefined && f.value !== null && f.type !== 'secret') {
            vals[f.key] = String(f.value);
          }
        });
      }
      if (ch.name === 'dingtalk') {
        vals.mode = vals.mode === 'stream' ? 'stream' : 'webhook';
      }
      this.formValues = vals;
      this.showAdvanced = false;
      this.showBusinessApi = false;
      this.setupStep = ch.configured ? 3 : 1;
      this.testPassed = !!ch.configured;
      this.resetQR();
      // Auto-start QR flow for QR-type channels
      if (ch.setup_type === 'qr') {
        this.startQR();
      }
    },

    updateDingTalkMode(mode) {
      if (!this.isDingTalkChannel()) return;
      this.formValues.mode = mode === 'stream' ? 'stream' : 'webhook';
      if (this.formValues.mode === 'stream') {
        delete this.formValues.access_token_env;
        delete this.formValues.secret_env;
        delete this.formValues.webhook_port;
      } else {
        delete this.formValues.client_id_env;
        delete this.formValues.client_secret_env;
      }
    },

    buildChannelPayload() {
      if (!this.isDingTalkChannel()) {
        return { fields: this.formValues };
      }
      var mode = this.dingTalkMode();
      var payloadFields = { mode: mode };
      if (this.formValues.default_agent !== undefined && this.formValues.default_agent !== null) {
        payloadFields.default_agent = this.formValues.default_agent;
      }
      if (mode === 'stream') {
        if (this.formValues.client_id_env !== undefined) payloadFields.client_id_env = this.formValues.client_id_env;
        if (this.formValues.client_secret_env !== undefined) payloadFields.client_secret_env = this.formValues.client_secret_env;
      } else {
        if (this.formValues.access_token_env !== undefined) payloadFields.access_token_env = this.formValues.access_token_env;
        if (this.formValues.secret_env !== undefined) payloadFields.secret_env = this.formValues.secret_env;
        if (this.formValues.webhook_port !== undefined) payloadFields.webhook_port = this.formValues.webhook_port;
      }
      return { fields: payloadFields };
    },

    // ── QR Code Flow (WhatsApp Web style) ──────────────────────────

    resetQR() {
      this.qr = {
        loading: false, available: false, dataUrl: '', sessionId: '',
        message: '', help: '', connected: false, expired: false, error: ''
      };
      if (this.qrPollTimer) { clearInterval(this.qrPollTimer); this.qrPollTimer = null; }
    },

    async startQR() {
      this.qr.loading = true;
      this.qr.error = '';
      this.qr.connected = false;
      this.qr.expired = false;
      try {
        var result = await OpenFangAPI.post('/api/channels/whatsapp/qr/start', {});
        this.qr.available = result.available || false;
        this.qr.dataUrl = result.qr_data_url || '';
        this.qr.sessionId = result.session_id || '';
        this.qr.message = result.message || '';
        this.qr.help = result.help || '';
        this.qr.connected = result.connected || false;
        if (this.qr.available && this.qr.dataUrl && !this.qr.connected) {
          this.pollQR();
        }
        if (this.qr.connected) {
          OpenFangToast.success('WhatsApp connected!');
          await this.refreshStatus();
        }
      } catch(e) {
        this.qr.error = e.message || 'Could not start QR login';
      }
      this.qr.loading = false;
    },

    pollQR() {
      var self = this;
      if (this.qrPollTimer) clearInterval(this.qrPollTimer);
      this.qrPollTimer = setInterval(async function() {
        try {
          var result = await OpenFangAPI.get('/api/channels/whatsapp/qr/status?session_id=' + encodeURIComponent(self.qr.sessionId));
          if (result.connected) {
            clearInterval(self.qrPollTimer);
            self.qrPollTimer = null;
            self.qr.connected = true;
            self.qr.message = result.message || 'Connected!';
            OpenFangToast.success('WhatsApp linked successfully!');
            await self.refreshStatus();
          } else if (result.expired) {
            clearInterval(self.qrPollTimer);
            self.qrPollTimer = null;
            self.qr.expired = true;
            self.qr.message = 'QR code expired. Click to generate a new one.';
          } else {
            self.qr.message = result.message || 'Waiting for scan...';
          }
        } catch(e) { /* silent retry */ }
      }, 3000);
    },

    // ── Standard Form Flow ─────────────────────────────────────────

    async saveChannel() {
      if (!this.setupModal) return;
      var name = this.setupModal.name;
      this.configuring = true;
      try {
        await OpenFangAPI.post('/api/channels/' + name + '/configure', this.buildChannelPayload());
        this.setupStep = 2;
        // Auto-test after save
        try {
          var testResult = await OpenFangAPI.post('/api/channels/' + name + '/test', {});
          if (testResult.status === 'ok') {
            this.testPassed = true;
            this.setupStep = 3;
            OpenFangToast.success(this.setupModal.display_name + ' activated!');
          } else {
            OpenFangToast.success(this.setupModal.display_name + ' saved. ' + (testResult.message || ''));
          }
        } catch(te) {
          OpenFangToast.success(this.setupModal.display_name + ' saved. Test to verify connection.');
        }
        await this.refreshStatus();
      } catch(e) {
        OpenFangToast.error('Failed: ' + (e.message || 'Unknown error'));
      }
      this.configuring = false;
    },

    async removeChannel() {
      if (!this.setupModal) return;
      var name = this.setupModal.name;
      var displayName = this.setupModal.display_name;
      var self = this;
      OpenFangToast.confirm('Remove Channel', 'Remove ' + displayName + ' configuration? This will deactivate the channel.', async function() {
        try {
          await OpenFangAPI.delete('/api/channels/' + name + '/configure');
          OpenFangToast.success(displayName + ' removed and deactivated.');
          await self.refreshStatus();
          self.setupModal = null;
        } catch(e) {
          OpenFangToast.error('Failed: ' + (e.message || 'Unknown error'));
        }
      });
    },

    async testChannel() {
      if (!this.setupModal) return;
      var name = this.setupModal.name;
      this.testing[name] = true;
      try {
        var result = await OpenFangAPI.post('/api/channels/' + name + '/test', {});
        if (result.status === 'ok') {
          this.testPassed = true;
          this.setupStep = 3;
          OpenFangToast.success(result.message);
        } else {
          OpenFangToast.error(result.message);
        }
      } catch(e) {
        OpenFangToast.error('Test failed: ' + (e.message || 'Unknown error'));
      }
      this.testing[name] = false;
    },

    async copyConfig(ch) {
      var tpl = ch ? ch.config_template : (this.setupModal ? this.setupModal.config_template : '');
      if (!tpl) return;
      try {
        await navigator.clipboard.writeText(tpl);
        OpenFangToast.success('Copied to clipboard');
      } catch(e) {
        OpenFangToast.error('Copy failed');
      }
    },

    destroy() {
      if (this.pollTimer) { clearInterval(this.pollTimer); this.pollTimer = null; }
      if (this.qrPollTimer) { clearInterval(this.qrPollTimer); this.qrPollTimer = null; }
    }
  };
}
