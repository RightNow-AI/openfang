// Operis Overview Dashboard — Landing page with system stats + provider status
'use strict';

function overviewPage() {
  return {
    health: {},
    status: {},
    usageSummary: {},
    recentAudit: [],
    channels: [],
    providers: [],
    mcpServers: [],
    skillCount: 0,
    loading: true,
    loadError: '',
    refreshTimer: null,
    lastRefresh: null,

    async loadOverview() {
      this.loading = true;
      this.loadError = '';
      try {
        await Promise.all([
          this.loadHealth(),
          this.loadStatus(),
          this.loadUsage(),
          this.loadAudit(),
          this.loadChannels(),
          this.loadProviders(),
          this.loadMcpServers(),
          this.loadSkills()
        ]);
        this.lastRefresh = Date.now();
      } catch(e) {
        this.loadError = e.message || 'Could not load overview data.';
      }
      this.loading = false;
    },

    async loadData() { return this.loadOverview(); },

    // Silent background refresh (no loading spinner)
    async silentRefresh() {
      try {
        await Promise.all([
          this.loadHealth(),
          this.loadStatus(),
          this.loadUsage(),
          this.loadAudit(),
          this.loadChannels(),
          this.loadProviders(),
          this.loadMcpServers(),
          this.loadSkills()
        ]);
        this.lastRefresh = Date.now();
      } catch(e) { /* silent */ }
    },

    startAutoRefresh() {
      this.stopAutoRefresh();
      this.refreshTimer = setInterval(() => this.silentRefresh(), 30000);
    },

    stopAutoRefresh() {
      if (this.refreshTimer) {
        clearInterval(this.refreshTimer);
        this.refreshTimer = null;
      }
    },

    async loadHealth() {
      try {
        this.health = await OperisAPI.get('/api/health');
      } catch(e) { this.health = { status: 'unreachable' }; }
    },

    async loadStatus() {
      try {
        this.status = await OperisAPI.get('/api/status');
      } catch(e) { this.status = {}; throw e; }
    },

    async loadUsage() {
      try {
        var data = await OperisAPI.get('/api/usage');
        var agents = data.agents || [];
        var totalTokens = 0;
        var totalTools = 0;
        var totalCost = 0;
        agents.forEach(function(a) {
          totalTokens += (a.total_tokens || 0);
          totalTools += (a.tool_calls || 0);
          totalCost += (a.cost_usd || 0);
        });
        this.usageSummary = {
          total_tokens: totalTokens,
          total_tools: totalTools,
          total_cost: totalCost,
          agent_count: agents.length
        };
      } catch(e) {
        this.usageSummary = { total_tokens: 0, total_tools: 0, total_cost: 0, agent_count: 0 };
      }
    },

    async loadAudit() {
      try {
        var data = await OperisAPI.get('/api/audit/recent?n=8');
        this.recentAudit = data.entries || [];
      } catch(e) { this.recentAudit = []; }
    },

    async loadChannels() {
      try {
        var data = await OperisAPI.get('/api/channels');
        this.channels = (data.channels || []).filter(function(ch) { return ch.has_token; });
      } catch(e) { this.channels = []; }
    },

    async loadProviders() {
      try {
        var data = await OperisAPI.get('/api/providers');
        this.providers = data.providers || [];
      } catch(e) { this.providers = []; }
    },

    async loadMcpServers() {
      try {
        var data = await OperisAPI.get('/api/mcp/servers');
        this.mcpServers = data.servers || [];
      } catch(e) { this.mcpServers = []; }
    },

    async loadSkills() {
      try {
        var data = await OperisAPI.get('/api/skills');
        this.skillCount = (data.skills || []).length;
      } catch(e) { this.skillCount = 0; }
    },

    get configuredProviders() {
      return this.providers.filter(function(p) { return p.auth_status === 'configured'; });
    },

    get unconfiguredProviders() {
      return this.providers.filter(function(p) { return p.auth_status === 'not_set' || p.auth_status === 'missing'; });
    },

    get connectedMcp() {
      return this.mcpServers.filter(function(s) { return s.status === 'connected'; });
    },

    // Provider health badge color
    providerBadgeClass(p) {
      if (p.auth_status === 'configured') {
        if (p.health === 'cooldown' || p.health === 'open') return 'badge-warn';
        return 'badge-success';
      }
      if (p.auth_status === 'not_set' || p.auth_status === 'missing') return 'badge-muted';
      return 'badge-dim';
    },

    // Provider health tooltip
    providerTooltip(p) {
      if (p.health === 'cooldown') return p.display_name + ' \u2014 cooling down (rate limited)';
      if (p.health === 'open') return p.display_name + ' \u2014 circuit breaker open';
      if (p.auth_status === 'configured') return p.display_name + ' \u2014 ready';
      return p.display_name + ' \u2014 not configured';
    },

    // Audit action badge color
    actionBadgeClass(action) {
      if (!action) return 'badge-dim';
      if (action === 'AgentSpawn' || action === 'AuthSuccess') return 'badge-success';
      if (action === 'AgentKill' || action === 'AgentTerminated' || action === 'AuthFailure' || action === 'CapabilityDenied') return 'badge-error';
      if (action === 'RateLimited' || action === 'ToolInvoke') return 'badge-warn';
      return 'badge-created';
    },

    // ── Setup Checklist ──
    checklistDismissed: localStorage.getItem('of-checklist-dismissed') === 'true',

    get setupChecklist() {
      return [
        { key: 'provider', label: 'Configure an LLM provider', done: this.configuredProviders.length > 0, action: '#settings' },
        { key: 'agent', label: 'Create your first agent', done: (Alpine.store('app').agents || []).length > 0, action: '#agents' },
        { key: 'chat', label: 'Send your first message', done: localStorage.getItem('of-first-msg') === 'true', action: '#chat' },
        { key: 'channel', label: 'Connect a messaging channel', done: this.channels.length > 0, action: '#channels' },
        { key: 'skill', label: 'Browse or install a skill', done: localStorage.getItem('of-skill-browsed') === 'true', action: '#skills' }
      ];
    },

    get setupProgress() {
      var done = this.setupChecklist.filter(function(item) { return item.done; }).length;
      return (done / 5) * 100;
    },

    get setupDoneCount() {
      return this.setupChecklist.filter(function(item) { return item.done; }).length;
    },

    dismissChecklist() {
      this.checklistDismissed = true;
      localStorage.setItem('of-checklist-dismissed', 'true');
    },

    formatUptime(secs) {
      if (!secs) return '-';
      var d = Math.floor(secs / 86400);
      var h = Math.floor((secs % 86400) / 3600);
      var m = Math.floor((secs % 3600) / 60);
      if (d > 0) return d + 'd ' + h + 'h';
      if (h > 0) return h + 'h ' + m + 'm';
      return m + 'm';
    },

    formatNumber(n) {
      if (!n) return '0';
      if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M';
      if (n >= 1000) return (n / 1000).toFixed(1) + 'K';
      return String(n);
    },

    formatCost(n) {
      if (!n || n === 0) return '$0.00';
      if (n < 0.01) return '<$0.01';
      return '$' + n.toFixed(2);
    },

    // Relative time formatting ("2m ago", "1h ago", "just now")
    timeAgo(timestamp) {
      if (!timestamp) return '';
      var now = Date.now();
      var ts = new Date(timestamp).getTime();
      var diff = Math.floor((now - ts) / 1000);
      if (diff < 10) return 'just now';
      if (diff < 60) return diff + 's ago';
      if (diff < 3600) return Math.floor(diff / 60) + 'm ago';
      if (diff < 86400) return Math.floor(diff / 3600) + 'h ago';
      return Math.floor(diff / 86400) + 'd ago';
    },

    // Map raw audit action names to user-friendly labels
    friendlyAction(action) {
      if (!action) return 'Unknown';
      var map = {
        'AgentSpawn': 'Agent Created',
        'AgentKill': 'Agent Stopped',
        'AgentTerminated': 'Agent Stopped',
        'ToolInvoke': 'Tool Used',
        'ToolResult': 'Tool Completed',
        'MessageReceived': 'Message In',
        'MessageSent': 'Response Sent',
        'SessionReset': 'Session Reset',
        'SessionCompact': 'Compacted',
        'ModelSwitch': 'Model Changed',
        'AuthAttempt': 'Login Attempt',
        'AuthSuccess': 'Login OK',
        'AuthFailure': 'Login Failed',
        'CapabilityDenied': 'Denied',
        'RateLimited': 'Rate Limited',
        'WorkflowRun': 'Workflow Run',
        'TriggerFired': 'Trigger Fired',
        'SkillInstalled': 'Skill Installed',
        'McpConnected': 'MCP Connected'
      };
      return map[action] || action.replace(/([A-Z])/g, ' $1').trim();
    },

    // Audit action icon (small inline SVG)
    actionIcon(action) {
      if (!action) return '';
      var icons = {
        'AgentSpawn': '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M12 8v8M8 12h8"/></svg>',
        'AgentKill': '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M15 9l-6 6M9 9l6 6"/></svg>',
        'AgentTerminated': '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M15 9l-6 6M9 9l6 6"/></svg>',
        'ToolInvoke': '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/></svg>',
        'MessageReceived': '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>',
        'MessageSent': '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z"/></svg>'
      };
      return icons[action] || '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/></svg>';
    },

    // Resolve agent UUID to name if possible
    agentName(agentId) {
      if (!agentId) return '-';
      var agents = Alpine.store('app').agents || [];
      var agent = agents.find(function(a) { return a.id === agentId; });
      return agent ? agent.name : agentId.substring(0, 8) + '\u2026';
    }
  };
}
