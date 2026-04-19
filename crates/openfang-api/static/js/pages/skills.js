// OpenFang Skills Page — OpenClaw/ClawHub ecosystem + local skills + MCP servers
'use strict';

function skillsPage() {
  return {
    tab: 'installed',
    skills: [],
    loading: true,
    loadError: '',

    // ClawHub state
    clawhubSearch: '',
    clawhubResults: [],
    clawhubBrowseResults: [],
    clawhubLoading: false,
    clawhubError: '',
    clawhubSort: 'trending',
    clawhubNextCursor: null,
    installingSlug: null,
    installResult: null,
    _searchTimer: null,
    _browseCache: {},    // { key: { ts, data } } client-side 60s cache
    _searchCache: {},

    // Skill detail modal
    skillDetail: null,
    detailLoading: false,
    showSkillCode: false,
    skillCode: '',
    skillCodeFilename: '',
    skillCodeLoading: false,

    // Skill config modal (local skill configuration from SKILL.md frontmatter)
    configSkill: null,          // skill object whose config is being edited
    configDeclared: {},         // { var_name: { description, env, default, required } }
    configResolved: {},         // { var_name: { value, source, is_secret } }
    configDraft: {},            // { var_name: user-edited string value }
    configRevealed: {},         // { var_name: bool } — toggle password reveal per row
    configLoading: false,
    configSaving: false,
    configError: '',

    // MCP servers
    mcpServers: [],
    mcpLoading: false,

    // Category definitions from the OpenClaw ecosystem (loaded from i18n)
    get categories() {
      var t = window.i18n ? window.i18n.t.bind(window.i18n) : function(k) { return k; };
      return [
        { id: 'coding', name: t('skills.cat_coding') || 'Coding & IDEs' },
        { id: 'git', name: t('skills.cat_git') || 'Git & GitHub' },
        { id: 'web', name: t('skills.cat_frontend') || 'Web & Frontend' },
        { id: 'devops', name: t('skills.cat_devops') || 'DevOps & Cloud' },
        { id: 'browser', name: t('skills.cat_browser') || 'Browser & Automation' },
        { id: 'search', name: t('skills.cat_search') || 'Search & Research' },
        { id: 'ai', name: t('skills.cat_ai') || 'AI & ML' },
        { id: 'data', name: t('skills.cat_data') || 'Data & Analytics' },
        { id: 'productivity', name: t('skills.cat_productivity') || 'Productivity' },
        { id: 'communication', name: t('skills.cat_communication') || 'Communication' },
        { id: 'media', name: t('skills.cat_media') || 'Media & Streaming' },
        { id: 'notes', name: t('skills.cat_notes') || 'Notes & PKM' },
        { id: 'security', name: t('skills.cat_security') || 'Security' },
        { id: 'cli', name: t('skills.cat_cli') || 'CLI Utilities' },
        { id: 'marketing', name: t('skills.cat_marketing') || 'Marketing & Sales' },
        { id: 'finance', name: t('skills.cat_finance') || 'Finance' },
        { id: 'smart-home', name: t('skills.cat_smarthome') || 'Smart Home & IoT' },
        { id: 'docs', name: t('skills.cat_docs') || 'Documentation' },
      ];
    },

    runtimeBadge: function(rt) {
      var r = (rt || '').toLowerCase();
      if (r === 'python' || r === 'py') return { text: 'PY', cls: 'runtime-badge-py' };
      if (r === 'node' || r === 'nodejs' || r === 'js' || r === 'javascript') return { text: 'JS', cls: 'runtime-badge-js' };
      if (r === 'wasm' || r === 'webassembly') return { text: 'WASM', cls: 'runtime-badge-wasm' };
      if (r === 'prompt_only' || r === 'prompt' || r === 'promptonly') return { text: 'PROMPT', cls: 'runtime-badge-prompt' };
      return { text: r.toUpperCase().substring(0, 4), cls: 'runtime-badge-prompt' };
    },

    sourceBadge: function(source) {
      if (!source) return { text: 'Local', cls: 'badge-dim' };
      switch (source.type) {
        case 'clawhub': return { text: 'ClawHub', cls: 'badge-info' };
        case 'openclaw': return { text: 'OpenClaw', cls: 'badge-info' };
        case 'bundled': return { text: 'Built-in', cls: 'badge-success' };
        default: return { text: 'Local', cls: 'badge-dim' };
      }
    },

    formatDownloads: function(n) {
      if (!n) return '0';
      if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M';
      if (n >= 1000) return (n / 1000).toFixed(1) + 'K';
      return n.toString();
    },

    async loadSkills() {
      this.loading = true;
      this.loadError = '';
      try {
        var data = await OpenFangAPI.get('/api/skills');
        this.skills = (data.skills || []).map(function(s) {
          return {
            name: s.name,
            description: s.description || '',
            version: s.version || '',
            author: s.author || '',
            runtime: s.runtime || 'unknown',
            tools_count: s.tools_count || 0,
            tags: s.tags || [],
            enabled: s.enabled !== false,
            source: s.source || { type: 'local' },
            has_prompt_context: !!s.has_prompt_context,
            config_declared_count: s.config_declared_count || 0
          };
        });
      } catch(e) {
        this.skills = [];
        this.loadError = e.message || 'Could not load skills.';
      }
      this.loading = false;
    },

    // ── Skill config editing ────────────────────────────────────────────
    async openSkillConfig(skill) {
      this.configSkill = skill;
      this.configDeclared = {};
      this.configResolved = {};
      this.configDraft = {};
      this.configRevealed = {};
      this.configError = '';
      this.configLoading = true;
      try {
        var data = await OpenFangAPI.get('/api/skills/' + encodeURIComponent(skill.name) + '/config');
        this.configDeclared = data.declared || {};
        this.configResolved = data.resolved || {};
        // Pre-populate draft values only for vars the user has already
        // overridden — never copy redacted responses back into inputs or
        // they'd be re-saved as "****redacted****" strings.
        var names = Object.keys(this.configDeclared);
        for (var i = 0; i < names.length; i++) {
          var n = names[i];
          var res = this.configResolved[n] || {};
          if (res.source === 'user' && !res.is_secret) {
            this.configDraft[n] = res.value == null ? '' : String(res.value);
          } else {
            this.configDraft[n] = '';
          }
        }
      } catch(e) {
        this.configError = e.message || 'Failed to load skill config.';
      }
      this.configLoading = false;
    },

    closeSkillConfig() {
      this.configSkill = null;
      this.configDeclared = {};
      this.configResolved = {};
      this.configDraft = {};
      this.configRevealed = {};
      this.configError = '';
    },

    configRowInvalid(name) {
      // A required var is invalid iff the user hasn't entered anything AND
      // no env/default resolves it. Source from the server tells us where
      // the current value came from; if it's "unresolved" and the draft is
      // blank, the save would write an empty string over the required var.
      var decl = this.configDeclared[name] || {};
      if (!decl.required) return false;
      var draft = (this.configDraft[name] || '').trim();
      if (draft) return false;
      var res = this.configResolved[name] || {};
      if (res.source === 'env' || res.source === 'default') return false;
      // If the user has an existing secret override we don't want to force
      // them to re-type it — treat that as "currently resolved".
      if (res.source === 'user') return false;
      return true;
    },

    hasInvalidConfig() {
      var names = Object.keys(this.configDeclared);
      for (var i = 0; i < names.length; i++) {
        if (this.configRowInvalid(names[i])) return true;
      }
      return false;
    },

    toggleReveal(name) {
      this.configRevealed[name] = !this.configRevealed[name];
    },

    sourceBadgeClass(source) {
      switch (source) {
        case 'user': return 'badge-success';
        case 'env': return 'badge-info';
        case 'default': return 'badge-dim';
        default: return 'badge-danger';
      }
    },

    sourceBadgeLabel(res) {
      if (!res) return 'unresolved';
      switch (res.source) {
        case 'user': return 'user override';
        case 'env': return 'env' + ((this.configDeclared[res.__name] && this.configDeclared[res.__name].env) ? ':' + this.configDeclared[res.__name].env : '');
        case 'default': return 'default';
        default: return 'unresolved';
      }
    },

    async saveSkillConfig() {
      if (!this.configSkill) return;
      if (this.hasInvalidConfig()) {
        OpenFangToast.error('Fill in all required variables before saving.');
        return;
      }
      this.configSaving = true;
      this.configError = '';
      // Only PUT values the user actually typed. Empty strings are dropped
      // so we don't silently clobber an env/default with "".
      var payload = {};
      var names = Object.keys(this.configDeclared);
      for (var i = 0; i < names.length; i++) {
        var n = names[i];
        var v = (this.configDraft[n] || '').trim();
        if (v.length > 0) payload[n] = v;
      }
      try {
        await OpenFangAPI.put('/api/skills/' + encodeURIComponent(this.configSkill.name) + '/config', { values: payload });
        OpenFangToast.success('Saved, reloading agents\u2026');
        // Refresh the modal contents so the new source/value shows up.
        var refreshed = this.configSkill;
        await this.loadSkills();
        this.closeSkillConfig();
        // Find the possibly-refreshed skill object and reopen.
        var self = this;
        var updated = this.skills.find(function(s) { return s.name === refreshed.name; });
        if (updated) await self.openSkillConfig(updated);
      } catch(e) {
        this.configError = e.message || 'Save failed.';
        OpenFangToast.error('Save failed: ' + (e.message || 'unknown error'));
      }
      this.configSaving = false;
    },

    async resetSkillConfigVar(name) {
      if (!this.configSkill) return;
      var decl = this.configDeclared[name] || {};
      var res = this.configResolved[name] || {};
      // If the server is already reporting a non-user source there's nothing
      // to remove; just clear the draft so the input disappears.
      if (res.source !== 'user') {
        this.configDraft[name] = '';
        return;
      }
      try {
        await OpenFangAPI.del('/api/skills/' + encodeURIComponent(this.configSkill.name) + '/config/' + encodeURIComponent(name));
        OpenFangToast.success('Reset ' + name);
        // Refresh modal state from server.
        var data = await OpenFangAPI.get('/api/skills/' + encodeURIComponent(this.configSkill.name) + '/config');
        this.configResolved = data.resolved || {};
        this.configDraft[name] = '';
      } catch(e) {
        var msg = e.message || 'Reset failed';
        if (msg.indexOf('required') !== -1 || msg.indexOf('409') !== -1) {
          OpenFangToast.error('Cannot reset: ' + decl.description + ' is required with no fallback.');
        } else {
          OpenFangToast.error('Reset failed: ' + msg);
        }
      }
    },

    get configDeclaredNames() {
      return Object.keys(this.configDeclared).sort();
    },

    async loadData() {
      await this.loadSkills();
    },

    // Debounced search — fires 350ms after user stops typing
    onSearchInput() {
      if (this._searchTimer) clearTimeout(this._searchTimer);
      var q = this.clawhubSearch.trim();
      if (!q) {
        this.clawhubResults = [];
        this.clawhubError = '';
        return;
      }
      var self = this;
      this._searchTimer = setTimeout(function() { self.searchClawHub(); }, 350);
    },

    // ClawHub search
    async searchClawHub() {
      if (!this.clawhubSearch.trim()) {
        this.clawhubResults = [];
        return;
      }
      this.clawhubLoading = true;
      this.clawhubError = '';
      try {
        var data = await OpenFangAPI.get('/api/clawhub/search?q=' + encodeURIComponent(this.clawhubSearch.trim()) + '&limit=20');
        this.clawhubResults = data.items || [];
        if (data.error) this.clawhubError = data.error;
      } catch(e) {
        this.clawhubResults = [];
        this.clawhubError = e.message || 'Search failed';
      }
      this.clawhubLoading = false;
    },

    // Clear search and go back to browse
    clearSearch() {
      this.clawhubSearch = '';
      this.clawhubResults = [];
      this.clawhubError = '';
      if (this._searchTimer) clearTimeout(this._searchTimer);
    },

    // ClawHub browse by sort (with 60s client-side cache)
    async browseClawHub(sort) {
      this.clawhubSort = sort || 'trending';
      var ckey = 'browse:' + this.clawhubSort;
      var cached = this._browseCache[ckey];
      if (cached && (Date.now() - cached.ts) < 60000) {
        this.clawhubBrowseResults = cached.data.items || [];
        this.clawhubNextCursor = cached.data.next_cursor || null;
        return;
      }
      this.clawhubLoading = true;
      this.clawhubError = '';
      this.clawhubNextCursor = null;
      try {
        var data = await OpenFangAPI.get('/api/clawhub/browse?sort=' + this.clawhubSort + '&limit=20');
        this.clawhubBrowseResults = data.items || [];
        this.clawhubNextCursor = data.next_cursor || null;
        if (data.error) this.clawhubError = data.error;
        this._browseCache[ckey] = { ts: Date.now(), data: data };
      } catch(e) {
        this.clawhubBrowseResults = [];
        this.clawhubError = e.message || 'Browse failed';
      }
      this.clawhubLoading = false;
    },

    // ClawHub load more results
    async loadMoreClawHub() {
      if (!this.clawhubNextCursor || this.clawhubLoading) return;
      this.clawhubLoading = true;
      try {
        var data = await OpenFangAPI.get('/api/clawhub/browse?sort=' + this.clawhubSort + '&limit=20&cursor=' + encodeURIComponent(this.clawhubNextCursor));
        this.clawhubBrowseResults = this.clawhubBrowseResults.concat(data.items || []);
        this.clawhubNextCursor = data.next_cursor || null;
      } catch(e) {
        // silently fail on load more
      }
      this.clawhubLoading = false;
    },

    // Show skill detail
    async showSkillDetail(slug) {
      this.detailLoading = true;
      this.skillDetail = null;
      this.installResult = null;
      try {
        var data = await OpenFangAPI.get('/api/clawhub/skill/' + encodeURIComponent(slug));
        this.skillDetail = data;
      } catch(e) {
        OpenFangToast.error('Failed to load skill details');
      }
      this.detailLoading = false;
    },

    closeDetail() {
      this.skillDetail = null;
      this.installResult = null;
      this.showSkillCode = false;
      this.skillCode = '';
      this.skillCodeFilename = '';
    },

    async viewSkillCode(slug) {
      if (this.showSkillCode) {
        this.showSkillCode = false;
        return;
      }
      this.skillCodeLoading = true;
      try {
        var data = await OpenFangAPI.get('/api/clawhub/skill/' + encodeURIComponent(slug) + '/code');
        this.skillCode = data.code || '';
        this.skillCodeFilename = data.filename || 'source';
        this.showSkillCode = true;
      } catch(e) {
        OpenFangToast.error('Could not load skill source code');
      }
      this.skillCodeLoading = false;
    },

    // Install from ClawHub
    async installFromClawHub(slug) {
      this.installingSlug = slug;
      this.installResult = null;
      try {
        var data = await OpenFangAPI.post('/api/clawhub/install', { slug: slug });
        this.installResult = data;
        if (data.warnings && data.warnings.length > 0) {
          OpenFangToast.success('Skill "' + data.name + '" installed with ' + data.warnings.length + ' warning(s)');
        } else {
          OpenFangToast.success('Skill "' + data.name + '" installed successfully');
        }
        // Update installed state in detail modal if open
        if (this.skillDetail && this.skillDetail.slug === slug) {
          this.skillDetail.installed = true;
        }
        await this.loadSkills();
      } catch(e) {
        var msg = e.message || 'Install failed';
        if (msg.includes('already_installed')) {
          OpenFangToast.error('Skill is already installed');
        } else if (msg.includes('SecurityBlocked')) {
          OpenFangToast.error('Skill blocked by security scan');
        } else {
          OpenFangToast.error('Install failed: ' + msg);
        }
      }
      this.installingSlug = null;
    },

    // Uninstall
    uninstallSkill: function(name) {
      var self = this;
      var t = window.i18n ? window.i18n.t.bind(window.i18n) : function(k) { return k; };
      OpenFangToast.confirm(
        t('skills.uninstall_skill') || 'Uninstall Skill',
        t('skills.uninstall_confirm') + ' "' + name + '"? This cannot be undone.',
        async function() {
          try {
            await OpenFangAPI.post('/api/skills/uninstall', { name: name });
            OpenFangToast.success('Skill "' + name + '" uninstalled');
            await self.loadSkills();
          } catch(e) {
            OpenFangToast.error('Failed to uninstall skill: ' + e.message);
          }
        }
      );
    },

    // Create prompt-only skill
    async createDemoSkill(skill) {
      try {
        await OpenFangAPI.post('/api/skills/create', {
          name: skill.name,
          description: skill.description,
          runtime: 'prompt_only',
          prompt_context: skill.prompt_context || skill.description
        });
        OpenFangToast.success('Skill "' + skill.name + '" created');
        this.tab = 'installed';
        await this.loadSkills();
      } catch(e) {
        OpenFangToast.error('Failed to create skill: ' + e.message);
      }
    },

    // Load MCP servers
    async loadMcpServers() {
      this.mcpLoading = true;
      try {
        var data = await OpenFangAPI.get('/api/mcp/servers');
        this.mcpServers = data;
      } catch(e) {
        this.mcpServers = { configured: [], connected: [], total_configured: 0, total_connected: 0 };
      }
      this.mcpLoading = false;
    },

    // Category search on ClawHub
    searchCategory: function(cat) {
      this.clawhubSearch = cat.name;
      this.searchClawHub();
    },

    // Quick start skills (prompt-only, zero deps)
    quickStartSkills: [
      { name: 'code-review-guide', description: 'Adds code review best practices and checklist to agent context.', prompt_context: 'You are an expert code reviewer. When reviewing code:\n1. Check for bugs and logic errors\n2. Evaluate code style and readability\n3. Look for security vulnerabilities\n4. Suggest performance improvements\n5. Verify error handling\n6. Check test coverage' },
      { name: 'writing-style', description: 'Configurable writing style guide for content generation.', prompt_context: 'Follow these writing guidelines:\n- Use clear, concise language\n- Prefer active voice over passive voice\n- Keep paragraphs short (3-4 sentences)\n- Use bullet points for lists\n- Maintain consistent tone throughout' },
      { name: 'api-design', description: 'REST API design patterns and conventions.', prompt_context: 'When designing REST APIs:\n- Use nouns for resources, not verbs\n- Use HTTP methods correctly (GET, POST, PUT, DELETE)\n- Return appropriate status codes\n- Use pagination for list endpoints\n- Version your API\n- Document all endpoints' },
      { name: 'security-checklist', description: 'OWASP-aligned security review checklist.', prompt_context: 'Security review checklist (OWASP aligned):\n- Input validation on all user inputs\n- Output encoding to prevent XSS\n- Parameterized queries to prevent SQL injection\n- Authentication and session management\n- Access control checks\n- CSRF protection\n- Security headers\n- Error handling without information leakage' },
    ],

    // Check if skill is installed by slug
    isSkillInstalled: function(slug) {
      return this.skills.some(function(s) {
        return s.source && s.source.type === 'clawhub' && s.source.slug === slug;
      });
    },

    // Check if skill is installed by name
    isSkillInstalledByName: function(name) {
      return this.skills.some(function(s) { return s.name === name; });
    },
  };
}
