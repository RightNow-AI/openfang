// OpenFang Analytics Page — Full usage analytics with per-model and per-agent breakdowns
// Includes Cost Dashboard with donut chart, bar chart, projections, and provider breakdown.
'use strict';

function analyticsPage() {
  return {
    tab: 'summary',
    summary: {},
    byModel: [],
    byAgent: [],
    loading: true,
    loadError: '',

    // Cost tab state
    dailyCosts: [],
    todayCost: 0,
    firstEventDate: null,

    // Chart colors for providers (stable palette)
    _chartColors: [
      '#FF5C00', '#3B82F6', '#10B981', '#F59E0B', '#8B5CF6',
      '#EC4899', '#06B6D4', '#EF4444', '#84CC16', '#F97316',
      '#6366F1', '#14B8A6', '#E11D48', '#A855F7', '#22D3EE'
    ],

    async loadUsage() {
      this.loading = true;
      this.loadError = '';
      try {
        await Promise.all([
          this.loadSummary(),
          this.loadByModel(),
          this.loadByAgent(),
          this.loadDailyCosts()
        ]);
      } catch(e) {
        this.loadError = e.message || 'Could not load usage data.';
      }
      this.loading = false;
    },

    async loadData() { return this.loadUsage(); },

    async loadSummary() {
      try {
        this.summary = await OpenFangAPI.get('/api/usage/summary');
      } catch(e) {
        this.summary = { total_input_tokens: 0, total_output_tokens: 0, total_cost_usd: 0, call_count: 0, total_tool_calls: 0 };
        throw e;
      }
    },

    async loadByModel() {
      try {
        var data = await OpenFangAPI.get('/api/usage/by-model');
        this.byModel = data.models || [];
      } catch(e) { this.byModel = []; }
    },

    async loadByAgent() {
      try {
        var data = await OpenFangAPI.get('/api/usage');
        this.byAgent = data.agents || [];
      } catch(e) { this.byAgent = []; }
    },

    async loadDailyCosts() {
      try {
        var data = await OpenFangAPI.get('/api/usage/daily');
        this.dailyCosts = data.days || [];
        this.todayCost = data.today_cost_usd || 0;
        this.firstEventDate = data.first_event_date || null;
      } catch(e) {
        this.dailyCosts = [];
        this.todayCost = 0;
        this.firstEventDate = null;
      }
    },

    formatTokens(n) {
      if (!n) return '0';
      if (n >= 1000000) return (n / 1000000).toFixed(2) + 'M';
      if (n >= 1000) return (n / 1000).toFixed(1) + 'K';
      return String(n);
    },

    formatCost(c) {
      if (!c) return '$0.00';
      if (c < 0.01) return '$' + c.toFixed(4);
      return '$' + c.toFixed(2);
    },

    maxTokens() {
      var max = 0;
      this.byModel.forEach(function(m) {
        var t = (m.total_input_tokens || 0) + (m.total_output_tokens || 0);
        if (t > max) max = t;
      });
      return max || 1;
    },

    barWidth(m) {
      var t = (m.total_input_tokens || 0) + (m.total_output_tokens || 0);
      return Math.max(2, Math.round((t / this.maxTokens()) * 100)) + '%';
    },

    // ── Cost tab helpers ──

    avgCostPerMessage() {
      var count = this.summary.call_count || 0;
      if (count === 0) return 0;
      return (this.summary.total_cost_usd || 0) / count;
    },

    projectedMonthlyCost() {
      if (!this.firstEventDate || !this.summary.total_cost_usd) return 0;
      var first = new Date(this.firstEventDate);
      var now = new Date();
      var diffMs = now.getTime() - first.getTime();
      var diffDays = diffMs / (1000 * 60 * 60 * 24);
      if (diffDays < 1) diffDays = 1;
      return (this.summary.total_cost_usd / diffDays) * 30;
    },

    // ── Provider aggregation from byModel data ──

    costByProvider() {
      var providerMap = {};
      var self = this;
      this.byModel.forEach(function(m) {
        var provider = self._extractProvider(m.model);
        if (!providerMap[provider]) {
          providerMap[provider] = { provider: provider, cost: 0, tokens: 0, calls: 0 };
        }
        providerMap[provider].cost += (m.total_cost_usd || 0);
        providerMap[provider].tokens += (m.total_input_tokens || 0) + (m.total_output_tokens || 0);
        providerMap[provider].calls += (m.call_count || 0);
      });
      var result = [];
      for (var key in providerMap) {
        if (providerMap.hasOwnProperty(key)) {
          result.push(providerMap[key]);
        }
      }
      result.sort(function(a, b) { return b.cost - a.cost; });
      return result;
    },

    _extractProvider(modelName) {
      if (!modelName) return 'Unknown';
      var lower = modelName.toLowerCase();
      if (lower.indexOf('claude') !== -1 || lower.indexOf('haiku') !== -1 || lower.indexOf('sonnet') !== -1 || lower.indexOf('opus') !== -1) return 'Anthropic';
      if (lower.indexOf('gemini') !== -1 || lower.indexOf('gemma') !== -1) return 'Google';
      if (lower.indexOf('gpt') !== -1 || lower.indexOf('o1') !== -1 || lower.indexOf('o3') !== -1 || lower.indexOf('o4') !== -1) return 'OpenAI';
      if (lower.indexOf('llama') !== -1 || lower.indexOf('mixtral') !== -1 || lower.indexOf('groq') !== -1) return 'Groq';
      if (lower.indexOf('deepseek') !== -1) return 'DeepSeek';
      if (lower.indexOf('mistral') !== -1) return 'Mistral';
      if (lower.indexOf('command') !== -1 || lower.indexOf('cohere') !== -1) return 'Cohere';
      if (lower.indexOf('grok') !== -1) return 'xAI';
      if (lower.indexOf('jamba') !== -1) return 'AI21';
      if (lower.indexOf('qwen') !== -1) return 'Together';
      return 'Other';
    },

    // ── Donut chart (stroke-dasharray on circles) ──

    donutSegments() {
      var providers = this.costByProvider();
      var total = 0;
      var colors = this._chartColors;
      providers.forEach(function(p) { total += p.cost; });
      if (total === 0) return [];

      var segments = [];
      var offset = 0;
      var circumference = 2 * Math.PI * 60; // r=60
      for (var i = 0; i < providers.length; i++) {
        var pct = providers[i].cost / total;
        var dashLen = pct * circumference;
        segments.push({
          provider: providers[i].provider,
          cost: providers[i].cost,
          percent: Math.round(pct * 100),
          color: colors[i % colors.length],
          dasharray: dashLen + ' ' + (circumference - dashLen),
          dashoffset: -offset,
          circumference: circumference
        });
        offset += dashLen;
      }
      return segments;
    },

    donutSegmentsSvg() {
      var segments = this.donutSegments();
      if (!segments.length) return '';

      function escapeXml(value) {
        return String(value)
          .replace(/&/g, '&amp;')
          .replace(/</g, '&lt;')
          .replace(/>/g, '&gt;')
          .replace(/"/g, '&quot;')
          .replace(/'/g, '&apos;');
      }

      var out = [];
      for (var i = 0; i < segments.length; i++) {
        var seg = segments[i];
        var title = seg.provider + ': ' + seg.percent + '% (' + this.formatCost(seg.cost) + ')';
        out.push(
          '<circle cx="80" cy="80" r="60" fill="none" stroke="' + escapeXml(seg.color) + '" stroke-width="24" stroke-dasharray="' + escapeXml(seg.dasharray) + '" stroke-dashoffset="' + escapeXml(seg.dashoffset) + '" transform="rotate(-90 80 80)" class="donut-segment">' +
          '<title>' + escapeXml(title) + '</title>' +
          '</circle>'
        );
      }

      return out.join('');
    },

    // ── Bar chart (last 7 days) ──

    barChartData() {
      var days = this.dailyCosts;
      if (!days || days.length === 0) return [];
      var maxCost = 0;
      days.forEach(function(d) { if (d.cost_usd > maxCost) maxCost = d.cost_usd; });
      if (maxCost === 0) maxCost = 1;

      var dayNames = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
      var result = [];
      for (var i = 0; i < days.length; i++) {
        var d = new Date(days[i].date + 'T12:00:00');
        var dayName = dayNames[d.getDay()] || '?';
        var heightPct = Math.max(2, Math.round((days[i].cost_usd / maxCost) * 120));
        result.push({
          date: days[i].date,
          dayName: dayName,
          cost: days[i].cost_usd,
          tokens: days[i].tokens,
          calls: days[i].calls,
          barHeight: heightPct
        });
      }
      return result;
    },

    barChartSvg() {
      var bars = this.barChartData();
      if (!bars.length) return '';

      function escapeXml(value) {
        return String(value)
          .replace(/&/g, '&amp;')
          .replace(/</g, '&lt;')
          .replace(/>/g, '&gt;')
          .replace(/"/g, '&quot;')
          .replace(/'/g, '&apos;');
      }

      var out = [];
      for (var i = 0; i < bars.length; i++) {
        var bar = bars[i];
        var x = i * 50 + 18;
        var labelX = i * 50 + 30;
        var y = 150 - bar.barHeight;
        var costLabelY = y - 4;
        var title = bar.date + ': ' + this.formatCost(bar.cost) + ' (' + bar.calls + ' calls)';

        out.push(
          '<g>' +
          '<rect x="' + x + '" y="' + y + '" width="24" height="' + bar.barHeight + '" rx="3" fill="var(--accent)" class="cost-bar" style="opacity:0.85">' +
          '<title>' + escapeXml(title) + '</title>' +
          '</rect>' +
          '<text x="' + labelX + '" y="166" text-anchor="middle" fill="var(--text-muted)" style="font-size:9px;font-family:var(--font-mono)">' + escapeXml(bar.dayName) + '</text>' +
          '<text x="' + labelX + '" y="' + costLabelY + '" text-anchor="middle" fill="var(--text-dim)" style="font-size:8px;font-family:var(--font-mono)">' + escapeXml(this.formatCost(bar.cost)) + '</text>' +
          '</g>'
        );
      }

      return out.join('');
    },

    // ── Cost by model table (sorted by cost descending) ──

    costByModelSorted() {
      var models = this.byModel.slice();
      models.sort(function(a, b) { return (b.total_cost_usd || 0) - (a.total_cost_usd || 0); });
      return models;
    },

    maxModelCost() {
      var max = 0;
      this.byModel.forEach(function(m) {
        if ((m.total_cost_usd || 0) > max) max = m.total_cost_usd;
      });
      return max || 1;
    },

    costBarWidth(m) {
      return Math.max(2, Math.round(((m.total_cost_usd || 0) / this.maxModelCost()) * 100)) + '%';
    },

    modelTier(modelName) {
      if (!modelName) return 'unknown';
      var lower = modelName.toLowerCase();
      if (lower.indexOf('opus') !== -1 || lower.indexOf('o1') !== -1 || lower.indexOf('o3') !== -1 || lower.indexOf('deepseek-r1') !== -1) return 'frontier';
      if (lower.indexOf('sonnet') !== -1 || lower.indexOf('gpt-4') !== -1 || lower.indexOf('gemini-2.5') !== -1 || lower.indexOf('gemini-1.5-pro') !== -1) return 'smart';
      if (lower.indexOf('haiku') !== -1 || lower.indexOf('gpt-3.5') !== -1 || lower.indexOf('flash') !== -1 || lower.indexOf('mixtral') !== -1) return 'balanced';
      if (lower.indexOf('llama') !== -1 || lower.indexOf('groq') !== -1 || lower.indexOf('gemma') !== -1) return 'fast';
      return 'balanced';
    }
  };
}
