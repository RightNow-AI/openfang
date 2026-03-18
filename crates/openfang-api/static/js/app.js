// FROZEN: Legacy Alpine SPA. Primary frontend → sdk/javascript/examples/nextjs-app-router/
// OpenFang App — Alpine.js init, hash router, global store
'use strict';

// Marked.js configuration
if (typeof marked !== 'undefined') {
  marked.setOptions({
    breaks: true,
    gfm: true,
    highlight: function(code, lang) {
      if (typeof hljs !== 'undefined' && lang && hljs.getLanguage(lang)) {
        try { return hljs.highlight(code, { language: lang }).value; } catch(e) {}
      }
      return code;
    }
  });
}

function escapeHtml(text) {
  var div = document.createElement('div');
  div.textContent = text || '';
  return div.innerHTML;
}

function renderMarkdown(text) {
  if (!text) return '';
  if (typeof marked !== 'undefined') {
    // Protect LaTeX blocks from marked.js mangling (underscores, backslashes, etc.)
    var latexBlocks = [];
    var protected_ = text;
    // Protect display math $$...$$ first (greedy across lines)
    protected_ = protected_.replace(/\$\$([\s\S]+?)\$\$/g, function(match) {
      var idx = latexBlocks.length;
      latexBlocks.push(match);
      return '\x00LATEX' + idx + '\x00';
    });
    // Protect inline math $...$ (single line, not empty, not starting/ending with space)
    protected_ = protected_.replace(/\$([^\s$](?:[^$]*[^\s$])?)\$/g, function(match) {
      var idx = latexBlocks.length;
      latexBlocks.push(match);
      return '\x00LATEX' + idx + '\x00';
    });
    // Protect \[...\] display math
    protected_ = protected_.replace(/\\\[([\s\S]+?)\\\]/g, function(match) {
      var idx = latexBlocks.length;
      latexBlocks.push(match);
      return '\x00LATEX' + idx + '\x00';
    });
    // Protect \(...\) inline math
    protected_ = protected_.replace(/\\\(([\s\S]+?)\\\)/g, function(match) {
      var idx = latexBlocks.length;
      latexBlocks.push(match);
      return '\x00LATEX' + idx + '\x00';
    });

    var html = marked.parse(protected_);
    // Restore LaTeX blocks
    for (var i = 0; i < latexBlocks.length; i++) {
      html = html.replace('\x00LATEX' + i + '\x00', latexBlocks[i]);
    }
    // Add copy buttons to code blocks
    html = html.replace(/<pre><code/g, '<pre><button class="copy-btn" onclick="copyCode(this)">Copy</button><code');
    // Open external links in new tab
    html = html.replace(/<a\s+href="(https?:\/\/[^"]*)"(?![^>]*target=)([^>]*)>/gi, '<a href="$1" target="_blank" rel="noopener"$2>');
    return html;
  }
  return escapeHtml(text);
}

// Render LaTeX math in the chat message container using KaTeX auto-render.
// Call this after new messages are inserted into the DOM.
function renderLatex(el) {
  if (typeof renderMathInElement !== 'function') return;
  var target = el || document.getElementById('messages');
  if (!target) return;
  try {
    renderMathInElement(target, {
      delimiters: [
        { left: '$$', right: '$$', display: true },
        { left: '\\[', right: '\\]', display: true },
        { left: '$', right: '$', display: false },
        { left: '\\(', right: '\\)', display: false }
      ],
      throwOnError: false,
      trust: false
    });
  } catch(e) { /* KaTeX render error — ignore gracefully */ }
}

function copyCode(btn) {
  var code = btn.nextElementSibling;
  if (code) {
    navigator.clipboard.writeText(code.textContent).then(function() {
      btn.textContent = 'Copied!';
      btn.classList.add('copied');
      setTimeout(function() { btn.textContent = 'Copy'; btn.classList.remove('copied'); }, 1500);
    });
  }
}

// Tool category icon SVGs — returns inline SVG for each tool category
function toolIcon(toolName) {
  if (!toolName) return '';
  var n = toolName.toLowerCase();
  var s = 'width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"';
  // File/directory operations
  if (n.indexOf('file_') === 0 || n.indexOf('directory_') === 0)
    return '<svg ' + s + '><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/><path d="M16 13H8"/><path d="M16 17H8"/></svg>';
  // Web/fetch
  if (n.indexOf('web_') === 0 || n.indexOf('link_') === 0)
    return '<svg ' + s + '><circle cx="12" cy="12" r="10"/><path d="M2 12h20"/><path d="M12 2a15 15 0 0 1 4 10 15 15 0 0 1-4 10 15 15 0 0 1-4-10 15 15 0 0 1 4-10z"/></svg>';
  // Shell/exec
  if (n.indexOf('shell') === 0 || n.indexOf('exec_') === 0)
    return '<svg ' + s + '><polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/></svg>';
  // Agent operations
  if (n.indexOf('agent_') === 0)
    return '<svg ' + s + '><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></svg>';
  // Memory/knowledge
  if (n.indexOf('memory_') === 0 || n.indexOf('knowledge_') === 0)
    return '<svg ' + s + '><path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z"/><path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z"/></svg>';
  // Cron/schedule
  if (n.indexOf('cron_') === 0 || n.indexOf('schedule_') === 0)
    return '<svg ' + s + '><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>';
  // Browser/playwright
  if (n.indexOf('browser_') === 0 || n.indexOf('playwright_') === 0)
    return '<svg ' + s + '><rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8"/><path d="M12 17v4"/></svg>';
  // Container/docker
  if (n.indexOf('container_') === 0 || n.indexOf('docker_') === 0)
    return '<svg ' + s + '><path d="M22 12H2"/><path d="M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/></svg>';
  // Image/media
  if (n.indexOf('image_') === 0 || n.indexOf('tts_') === 0)
    return '<svg ' + s + '><rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>';
  // Hand tools
  if (n.indexOf('hand_') === 0)
    return '<svg ' + s + '><path d="M18 11V6a2 2 0 0 0-2-2 2 2 0 0 0-2 2"/><path d="M14 10V4a2 2 0 0 0-2-2 2 2 0 0 0-2 2v6"/><path d="M10 10.5V6a2 2 0 0 0-2-2 2 2 0 0 0-2 2v8"/><path d="M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.9-5.7-2.4L3.4 16a2 2 0 0 1 3.2-2.4L8 15"/></svg>';
  // Task/collab
  if (n.indexOf('task_') === 0)
    return '<svg ' + s + '><path d="M9 11l3 3L22 4"/><path d="M21 12v7a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2h11"/></svg>';
  // Default — wrench
  return '<svg ' + s + '><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/></svg>';
}

// Alpine.js global store
document.addEventListener('alpine:init', function() {
  // Restore saved bearer token on load
  var savedKey = localStorage.getItem('openfang-auth-token') || localStorage.getItem('openfang-api-key');
  if (savedKey) OpenFangAPI.setAuthToken(savedKey);

  Alpine.store('app', {
    agents: [],
    connected: false,
    booting: true,
    wsConnected: false,
    connectionState: 'connected',
    lastError: '',
    version: '0.1.0',
    agentCount: 0,
    pendingAgent: null,
    focusMode: localStorage.getItem('openfang-focus') === 'true',
    showOnboarding: false,
    showAuthPrompt: false,
    authStatus: { required: false, mode: 'open', providers: { api_key: false, jwt: false, github_device_flow: false } },
    authUser: null,
    oauthFlow: { provider: 'github', pollId: '', userCode: '', verificationUri: '', interval: 5, polling: false, error: '' },

    saveAuthToken(token) {
      OpenFangAPI.setAuthToken(token);
      localStorage.setItem('openfang-auth-token', token);
      localStorage.removeItem('openfang-api-key');
    },

    clearStoredAuthToken() {
      OpenFangAPI.setAuthToken('');
      localStorage.removeItem('openfang-auth-token');
      localStorage.removeItem('openfang-api-key');
      this.authUser = null;
    },

    async loadAuthStatus() {
      try {
        var status = await OpenFangAPI.get('/api/auth/status');
        this.authStatus = status || this.authStatus;
        this.authUser = status && status.current_user ? status.current_user : null;
      } catch (e) {
        this.authStatus = this.authStatus || { required: false, mode: 'open', providers: {} };
      }
    },

    canUseGitHubOAuth() {
      return !!(this.authStatus && this.authStatus.providers && this.authStatus.providers.github_device_flow);
    },

    toggleFocusMode() {
      this.focusMode = !this.focusMode;
      localStorage.setItem('openfang-focus', this.focusMode);
    },

    async refreshAgents() {
      try {
        var agents = await OpenFangAPI.get('/api/agents');
        this.agents = Array.isArray(agents) ? agents : [];
        this.agentCount = this.agents.length;
      } catch(e) { /* silent */ }
    },

    async checkStatus() {
      try {
        var results = await Promise.all([
          OpenFangAPI.get('/api/health'),
          OpenFangAPI.get('/api/version')
        ]);
        var health = results[0] || {};
        var version = results[1] || {};
        this.connected = true;
        this.booting = false;
        this.lastError = '';
        this.version = version.version || health.version || '0.1.0';
      } catch(e) {
        this.connected = false;
        this.lastError = e.message || 'Unknown error';
        console.warn('[OpenFang] Status check failed:', e.message);
      }
    },

    async checkOnboarding() {
      if (localStorage.getItem('openfang-onboarded')) return;
      try {
        var config = await OpenFangAPI.get('/api/config');
        var apiKey = config && config.api_key;
        var noKey = !apiKey || apiKey === 'not set' || apiKey === '';
        if (noKey && this.agentCount === 0) {
          this.showOnboarding = true;
        }
      } catch(e) {
        // If config endpoint fails, still show onboarding if no agents
        if (this.agentCount === 0) this.showOnboarding = true;
      }
    },

    dismissOnboarding() {
      this.showOnboarding = false;
      localStorage.setItem('openfang-onboarded', 'true');
    },

    async checkAuth() {
      await this.loadAuthStatus();
      try {
        // Use a protected endpoint (not in the public allowlist) to detect
        // whether the server requires an API key.
        await OpenFangAPI.get('/api/tools');
        await this.loadAuthStatus();
        this.showAuthPrompt = false;
      } catch(e) {
        if (e.message && (e.message.indexOf('Not authorized') >= 0 || e.message.indexOf('401') >= 0 || e.message.indexOf('Missing Authorization') >= 0 || e.message.indexOf('Unauthorized') >= 0)) {
          // Only show prompt if we don't already have a saved key
          var saved = localStorage.getItem('openfang-auth-token') || localStorage.getItem('openfang-api-key');
          if (saved) {
            // Saved key might be stale — clear it and show prompt
            this.clearStoredAuthToken();
          }
          this.showAuthPrompt = true;
        }
      }
    },

    submitApiKey(key) {
      if (!key || !key.trim()) return;
      this.saveAuthToken(key.trim());
      this.showAuthPrompt = false;
      this.refreshAgents();
      this.loadAuthStatus();
    },

    clearApiKey() {
      this.clearStoredAuthToken();
    },

    async startGitHubOAuth() {
      this.oauthFlow.error = '';
      this.oauthFlow.polling = true;
      OpenFangOAuth.start();
    },

    // Kept only for legacy call-sites; now driven by OpenFangOAuth CustomEvents below
    async pollGitHubOAuth() {}
  });

  // Handle auth-required events dispatched by OpenFangAPI (framework-agnostic)
  document.addEventListener('openfang:auth-required', function() {
    var store = Alpine.store('app');
    if (store && !store.showAuthPrompt) store.showAuthPrompt = true;
  });

  // Handle GitHub OAuth events dispatched by OpenFangOAuth module
  document.addEventListener('openfang:oauth-started', function(e) {
    var store = Alpine.store('app');
    if (!store) return;
    store.oauthFlow.userCode = e.detail.userCode;
    store.oauthFlow.verificationUri = e.detail.verificationUri;
    store.oauthFlow.polling = true;
    store.oauthFlow.error = '';
  });

  document.addEventListener('openfang:oauth-success', async function(e) {
    var store = Alpine.store('app');
    if (!store) return;
    store.saveAuthToken(e.detail.token);
    store.authUser = e.detail.user || null;
    store.showAuthPrompt = false;
    store.oauthFlow = { provider: 'github', pollId: '', userCode: '', verificationUri: '', interval: 5, polling: false, error: '' };
    await store.loadAuthStatus();
    await store.refreshAgents();
    OpenFangToast.success('Signed in with GitHub');
  });

  document.addEventListener('openfang:oauth-error', function(e) {
    var store = Alpine.store('app');
    if (!store) return;
    store.oauthFlow.polling = false;
    store.oauthFlow.error = e.detail.message;
  });

  document.addEventListener('openfang:oauth-expired', function() {
    var store = Alpine.store('app');
    if (!store) return;
    store.oauthFlow.polling = false;
    store.oauthFlow.error = 'GitHub sign-in expired — please try again';
  });
});

// Main app component
function app() {
  return {
    page: 'agents',
    themeMode: localStorage.getItem('openfang-theme-mode') || 'system',
    theme: (() => {
      var mode = localStorage.getItem('openfang-theme-mode') || 'system';
      if (mode === 'system') return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
      return mode;
    })(),
    sidebarCollapsed: localStorage.getItem('openfang-sidebar') === 'collapsed',
    mobileMenuOpen: false,
    connected: false,
    wsConnected: false,
    version: '0.1.0',
    agentCount: 0,

    get agents() { return Alpine.store('app').agents; },

    init() {
      var self = this;

      // Sync theme to <html> on Alpine boot (aligns with inline script in <head>)
      document.documentElement.setAttribute('data-theme', this.theme);

      // Listen for OS theme changes (only matters when mode is 'system')
      window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function(e) {
        if (self.themeMode === 'system') {
          self.theme = e.matches ? 'dark' : 'light';
          document.documentElement.setAttribute('data-theme', self.theme);
        }
      });

      // Hash routing
      var validPages = ['today','inbox','agent-catalog','overview','agents','sessions','approvals','comms','workflows','scheduler','channels','skills','hands','analytics','logs','runtime','settings','wizard'];
      var pageRedirects = {
        'chat': 'agents',
        'templates': 'agents',
        'triggers': 'workflows',
        'cron': 'scheduler',
        'schedules': 'scheduler',
        'memory': 'sessions',
        'audit': 'logs',
        'security': 'settings',
        'peers': 'settings',
        'migration': 'settings',
        'usage': 'analytics',
        'approval': 'approvals'
      };
      function handleHash() {
        var hash = window.location.hash.replace('#', '') || 'today';
        if (pageRedirects[hash]) {
          hash = pageRedirects[hash];
          window.location.hash = hash;
        }
        if (validPages.indexOf(hash) >= 0) self.page = hash;
      }
      window.addEventListener('hashchange', handleHash);
      handleHash();

      // Keyboard shortcuts
      document.addEventListener('keydown', function(e) {
        // Ctrl+K — focus agent switch / go to agents
        if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
          e.preventDefault();
          self.navigate('agents');
        }
        // Ctrl+N — new agent
        if ((e.ctrlKey || e.metaKey) && e.key === 'n' && !e.shiftKey) {
          e.preventDefault();
          self.navigate('agents');
        }
        // Ctrl+Shift+F — toggle focus mode
        if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'F') {
          e.preventDefault();
          Alpine.store('app').toggleFocusMode();
        }
        // Escape — close mobile menu
        if (e.key === 'Escape') {
          self.mobileMenuOpen = false;
        }
      });

      // Connection state listener
      OpenFangAPI.onConnectionChange(function(state) {
        Alpine.store('app').connectionState = state;
      });

      // Initial data load
      this.pollStatus();
      Alpine.store('app').checkAuth().then(function() {
        if (!Alpine.store('app').showAuthPrompt) {
          Alpine.store('app').checkOnboarding();
        }
      });
      setInterval(function() { self.pollStatus(); }, 5000);
    },

    navigate(p) {
      this.page = p;
      window.location.hash = p;
      this.mobileMenuOpen = false;
    },

    setTheme(mode) {
      this.themeMode = mode;
      localStorage.setItem('openfang-theme-mode', mode);
      if (mode === 'system') {
        this.theme = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
      } else {
        this.theme = mode;
      }
      document.documentElement.setAttribute('data-theme', this.theme);
    },

    toggleTheme() {
      var modes = ['light', 'system', 'dark'];
      var next = modes[(modes.indexOf(this.themeMode) + 1) % modes.length];
      this.setTheme(next);
    },

    toggleSidebar() {
      this.sidebarCollapsed = !this.sidebarCollapsed;
      localStorage.setItem('openfang-sidebar', this.sidebarCollapsed ? 'collapsed' : 'expanded');
    },

    async pollStatus() {
      var store = Alpine.store('app');
      await store.checkStatus();
      await store.refreshAgents();
      this.connected = store.connected;
      this.version = store.version;
      this.agentCount = store.agentCount;
      this.wsConnected = OpenFangAPI.isWsConnected();
    }
  };
}
