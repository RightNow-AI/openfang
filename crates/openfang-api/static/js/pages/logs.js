// Operis Logs Page — Real-time log viewer (SSE streaming + polling fallback) + Audit Trail tab
'use strict';

function logsPage() {
  return {
    tab: 'live',
    // -- Live logs state --
    entries: [],
    levelFilter: '',
    textFilter: '',
    autoRefresh: true,
    hovering: false,
    loading: true,
    loadError: '',
    _pollTimer: null,

    // -- SSE streaming state --
    _eventSource: null,
    streamConnected: false,
    streamPaused: false,

    // -- Audit state --
    auditEntries: [],
    tipHash: '',
    chainValid: null,
    filterAction: '',
    auditLoading: false,
    auditLoadError: '',

    startStreaming: function() {
      var self = this;
      if (this._eventSource) { this._eventSource.close(); this._eventSource = null; }

      var url = '/api/logs/stream';
      var sep = '?';
      var token = OperisAPI.getToken();
      if (token) { url += sep + 'token=' + encodeURIComponent(token); sep = '&'; }

      try {
        this._eventSource = new EventSource(url);
      } catch(e) {
        // EventSource not supported or blocked; fall back to polling
        this.streamConnected = false;
        this.startPolling();
        return;
      }

      this._eventSource.onopen = function() {
        self.streamConnected = true;
        self.loading = false;
        self.loadError = '';
      };

      this._eventSource.onmessage = function(event) {
        if (self.streamPaused) return;
        try {
          var entry = JSON.parse(event.data);
          // Avoid duplicate entries by checking seq
          var dominated = false;
          for (var i = 0; i < self.entries.length; i++) {
            if (self.entries[i].seq === entry.seq) { dominated = true; break; }
          }
          if (!dominated) {
            self.entries.push(entry);
            // Cap at 500 entries (remove oldest)
            if (self.entries.length > 500) {
              self.entries.splice(0, self.entries.length - 500);
            }
            // Auto-scroll to bottom
            if (self.autoRefresh && !self.hovering) {
              self.$nextTick(function() {
                var el = document.getElementById('log-container');
                if (el) el.scrollTop = el.scrollHeight;
              });
            }
          }
        } catch(e) {
          // Ignore parse errors (heartbeat comments are not delivered to onmessage)
        }
      };

      this._eventSource.onerror = function() {
        self.streamConnected = false;
        if (self._eventSource) {
          self._eventSource.close();
          self._eventSource = null;
        }
        // Fall back to polling
        self.startPolling();
      };
    },

    startPolling: function() {
      var self = this;
      this.streamConnected = false;
      this.fetchLogs();
      if (this._pollTimer) clearInterval(this._pollTimer);
      this._pollTimer = setInterval(function() {
        if (self.autoRefresh && !self.hovering && self.tab === 'live' && !self.streamPaused) {
          self.fetchLogs();
        }
      }, 2000);
    },

    async fetchLogs() {
      if (this.loading) this.loadError = '';
      try {
        var data = await OperisAPI.get('/api/audit/recent?n=200');
        this.entries = data.entries || [];
        if (this.autoRefresh && !this.hovering) {
          this.$nextTick(function() {
            var el = document.getElementById('log-container');
            if (el) el.scrollTop = el.scrollHeight;
          });
        }
        if (this.loading) this.loading = false;
      } catch(e) {
        if (this.loading) {
          this.loadError = e.message || 'Could not load logs.';
          this.loading = false;
        }
      }
    },

    async loadData() {
      this.loading = true;
      return this.fetchLogs();
    },

    togglePause: function() {
      this.streamPaused = !this.streamPaused;
      if (!this.streamPaused && this.streamConnected) {
        // Resume: scroll to bottom
        var self = this;
        this.$nextTick(function() {
          var el = document.getElementById('log-container');
          if (el) el.scrollTop = el.scrollHeight;
        });
      }
    },

    clearLogs: function() {
      this.entries = [];
    },

    classifyLevel: function(action) {
      if (!action) return 'info';
      var a = action.toLowerCase();
      if (a.indexOf('error') !== -1 || a.indexOf('fail') !== -1 || a.indexOf('crash') !== -1) return 'error';
      if (a.indexOf('warn') !== -1 || a.indexOf('deny') !== -1 || a.indexOf('block') !== -1) return 'warn';
      return 'info';
    },

    get filteredEntries() {
      var self = this;
      var levelF = this.levelFilter;
      var textF = this.textFilter.toLowerCase();
      return this.entries.filter(function(e) {
        if (levelF && self.classifyLevel(e.action) !== levelF) return false;
        if (textF) {
          var haystack = ((e.action || '') + ' ' + (e.detail || '') + ' ' + (e.agent_id || '')).toLowerCase();
          if (haystack.indexOf(textF) === -1) return false;
        }
        return true;
      });
    },

    get connectionLabel() {
      if (this.streamPaused) return 'Paused';
      if (this.streamConnected) return 'Live';
      if (this._pollTimer) return 'Polling';
      return 'Disconnected';
    },

    get connectionClass() {
      if (this.streamPaused) return 'paused';
      if (this.streamConnected) return 'live';
      if (this._pollTimer) return 'polling';
      return 'disconnected';
    },

    exportLogs: function() {
      var lines = this.filteredEntries.map(function(e) {
        return new Date(e.timestamp).toISOString() + ' [' + e.action + '] ' + (e.detail || '');
      });
      var blob = new Blob([lines.join('\n')], { type: 'text/plain' });
      var url = URL.createObjectURL(blob);
      var a = document.createElement('a');
      a.href = url;
      a.download = 'operis-logs-' + new Date().toISOString().slice(0, 10) + '.txt';
      a.click();
      URL.revokeObjectURL(url);
    },

    // -- Audit methods --
    get filteredAuditEntries() {
      var self = this;
      if (!self.filterAction) return self.auditEntries;
      return self.auditEntries.filter(function(e) { return e.action === self.filterAction; });
    },

    async loadAudit() {
      this.auditLoading = true;
      this.auditLoadError = '';
      try {
        var data = await OperisAPI.get('/api/audit/recent?n=200');
        this.auditEntries = data.entries || [];
        this.tipHash = data.tip_hash || '';
      } catch(e) {
        this.auditEntries = [];
        this.auditLoadError = e.message || 'Could not load audit log.';
      }
      this.auditLoading = false;
    },

    auditAgentName: function(agentId) {
      if (!agentId) return '-';
      var agents = Alpine.store('app').agents || [];
      var agent = agents.find(function(a) { return a.id === agentId; });
      return agent ? agent.name : agentId.substring(0, 8) + '...';
    },

    friendlyAction: function(action) {
      if (!action) return 'Unknown';
      var map = {
        'AgentSpawn': 'Agent Created', 'AgentKill': 'Agent Stopped', 'AgentTerminated': 'Agent Stopped',
        'ToolInvoke': 'Tool Used', 'ToolResult': 'Tool Completed', 'AgentMessage': 'Message',
        'NetworkAccess': 'Network Access', 'ShellExec': 'Shell Command', 'FileAccess': 'File Access',
        'MemoryAccess': 'Memory Access', 'AuthAttempt': 'Login Attempt', 'AuthSuccess': 'Login Success',
        'AuthFailure': 'Login Failed', 'CapabilityDenied': 'Permission Denied', 'RateLimited': 'Rate Limited'
      };
      return map[action] || action.replace(/([A-Z])/g, ' $1').trim();
    },

    async verifyChain() {
      try {
        var data = await OperisAPI.get('/api/audit/verify');
        this.chainValid = data.valid === true;
        if (this.chainValid) {
          OperisToast.success('Audit chain verified — ' + (data.entries || 0) + ' entries valid');
        } else {
          OperisToast.error('Audit chain broken!');
        }
      } catch(e) {
        this.chainValid = false;
        OperisToast.error('Chain verification failed: ' + e.message);
      }
    },

    destroy: function() {
      if (this._eventSource) { this._eventSource.close(); this._eventSource = null; }
      if (this._pollTimer) { clearInterval(this._pollTimer); this._pollTimer = null; }
    }
  };
}
