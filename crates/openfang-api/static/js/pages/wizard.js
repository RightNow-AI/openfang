// OpenFang Setup Wizard — First-run guided setup (Provider + Agent + Channel)
'use strict';

/** Escape a string for use inside TOML triple-quoted strings ("""\n...\n"""). */
function wizardTomlMultilineEscape(s) {
  return s.replace(/\\/g, '\\\\').replace(/"""/g, '""\\"');
}

/** Escape a string for use inside a TOML basic (single-line) string ("..."). */
function wizardTomlBasicEscape(s) {
  return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '\\r').replace(/\t/g, '\\t');
}

function wizardPage() {
  return {
    step: 1,
    totalSteps: 6,
    loading: false,
    error: '',

    // Step 2: Provider setup
    providers: [],
    selectedProvider: '',
    apiKeyInput: '',
    testingProvider: false,
    testResult: null,
    savingKey: false,
    keySaved: false,

    // Step 3: Agent creation
    templates: [
      {
        id: 'assistant',
        name: 'General Assistant',
        description: 'A versatile helper for everyday tasks, answering questions, and providing recommendations.',
        icon: 'GA',
        category: 'General',
        provider: 'deepseek',
        model: 'deepseek-chat',
        profile: 'balanced',
        system_prompt: 'You are a helpful, friendly assistant. Provide clear, accurate, and concise responses. Ask clarifying questions when needed.'
      },
      {
        id: 'coder',
        name: 'Code Helper',
        description: 'A programming-focused agent that writes, reviews, and debugs code across multiple languages.',
        icon: 'CH',
        category: 'Development',
        provider: 'deepseek',
        model: 'deepseek-chat',
        profile: 'precise',
        system_prompt: 'You are an expert programmer. Help users write clean, efficient code. Explain your reasoning. Follow best practices and conventions for the language being used.'
      },
      {
        id: 'researcher',
        name: 'Researcher',
        description: 'An analytical agent that breaks down complex topics, synthesizes information, and provides cited summaries.',
        icon: 'RS',
        category: 'Research',
        provider: 'gemini',
        model: 'gemini-2.5-flash',
        profile: 'balanced',
        system_prompt: 'You are a research analyst. Break down complex topics into clear explanations. Provide structured analysis with key findings. Cite sources when available.'
      },
      {
        id: 'writer',
        name: 'Writer',
        description: 'A creative writing agent that helps with drafting, editing, and improving written content of all kinds.',
        icon: 'WR',
        category: 'Writing',
        provider: 'deepseek',
        model: 'deepseek-chat',
        profile: 'creative',
        system_prompt: 'You are a skilled writer and editor. Help users create polished content. Adapt your tone and style to match the intended audience. Offer constructive suggestions for improvement.'
      },
      {
        id: 'data-analyst',
        name: 'Data Analyst',
        description: 'A data-focused agent that helps analyze datasets, create queries, and interpret statistical results.',
        icon: 'DA',
        category: 'Development',
        provider: 'gemini',
        model: 'gemini-2.5-flash',
        profile: 'precise',
        system_prompt: 'You are a data analysis expert. Help users understand their data, write SQL/Python queries, and interpret results. Present findings clearly with actionable insights.'
      },
      {
        id: 'devops',
        name: 'DevOps Engineer',
        description: 'A systems-focused agent for CI/CD, infrastructure, Docker, and deployment troubleshooting.',
        icon: 'DO',
        category: 'Development',
        provider: 'deepseek',
        model: 'deepseek-chat',
        profile: 'precise',
        system_prompt: 'You are a DevOps engineer. Help with CI/CD pipelines, Docker, Kubernetes, infrastructure as code, and deployment. Prioritize reliability and security.'
      },
      {
        id: 'support',
        name: 'Customer Support',
        description: 'A professional, empathetic agent for handling customer inquiries and resolving issues.',
        icon: 'CS',
        category: 'Business',
        provider: 'groq',
        model: 'llama-3.3-70b-versatile',
        profile: 'balanced',
        system_prompt: 'You are a professional customer support representative. Be empathetic, patient, and solution-oriented. Acknowledge concerns before offering solutions. Escalate complex issues appropriately.'
      },
      {
        id: 'tutor',
        name: 'Tutor',
        description: 'A patient educational agent that explains concepts step-by-step and adapts to the learner\'s level.',
        icon: 'TU',
        category: 'General',
        provider: 'groq',
        model: 'llama-3.3-70b-versatile',
        profile: 'balanced',
        system_prompt: 'You are a patient and encouraging tutor. Explain concepts step by step, starting from fundamentals. Use analogies and examples. Check understanding before moving on. Adapt to the learner\'s pace.'
      },
      {
        id: 'api-designer',
        name: 'API Designer',
        description: 'An agent specialized in RESTful API design, OpenAPI specs, and integration architecture.',
        icon: 'AD',
        category: 'Development',
        provider: 'deepseek',
        model: 'deepseek-chat',
        profile: 'precise',
        system_prompt: 'You are an API design expert. Help users design clean, consistent RESTful APIs following best practices. Cover endpoint naming, request/response schemas, error handling, and versioning.'
      },
      {
        id: 'meeting-notes',
        name: 'Meeting Notes',
        description: 'Summarizes meeting transcripts into structured notes with action items and key decisions.',
        icon: 'MN',
        category: 'Business',
        provider: 'groq',
        model: 'llama-3.3-70b-versatile',
        profile: 'precise',
        system_prompt: 'You are a meeting summarizer. When given a meeting transcript or notes, produce a structured summary with: key decisions, action items (with owners), discussion highlights, and follow-up questions.'
      }
    ],
    selectedTemplate: 0,
    agentName: 'my-assistant',
    creatingAgent: false,
    createdAgent: null,

    // Step 3: Category filtering
    templateCategory: 'All',
    get templateCategories() {
      var cats = { 'All': true };
      this.templates.forEach(function(t) { if (t.category) cats[t.category] = true; });
      return Object.keys(cats);
    },
    get filteredTemplates() {
      var cat = this.templateCategory;
      if (cat === 'All') return this.templates;
      return this.templates.filter(function(t) { return t.category === cat; });
    },

    // Step 3: Profile/tool descriptions (loaded from i18n)
    get profileDescriptions() {
      return {
        minimal: { label: window.i18n ? window.i18n.t('wizard.profile_minimal') : 'Minimal', desc: window.i18n ? window.i18n.t('wizard.profile_minimal_desc') : 'Read-only file access' },
        coding: { label: window.i18n ? window.i18n.t('wizard.profile_coding') : 'Coding', desc: window.i18n ? window.i18n.t('wizard.profile_coding_desc') : 'Files + shell + web fetch' },
        research: { label: window.i18n ? window.i18n.t('wizard.profile_research') : 'Research', desc: window.i18n ? window.i18n.t('wizard.profile_research_desc') : 'Web search + file read/write' },
        balanced: { label: window.i18n ? window.i18n.t('wizard.profile_balanced') : 'Balanced', desc: window.i18n ? window.i18n.t('wizard.profile_balanced_desc') : 'General-purpose tool set' },
        precise: { label: window.i18n ? window.i18n.t('wizard.profile_precise') : 'Precise', desc: window.i18n ? window.i18n.t('wizard.profile_precise_desc') : 'Focused tool set for accuracy' },
        creative: { label: window.i18n ? window.i18n.t('wizard.profile_creative') : 'Creative', desc: window.i18n ? window.i18n.t('wizard.profile_creative_desc') : 'Full tools with creative emphasis' },
        full: { label: window.i18n ? window.i18n.t('wizard.profile_full') : 'Full', desc: window.i18n ? window.i18n.t('wizard.profile_full_desc') : 'All 35+ tools' }
      };
    },
    profileInfo: function(name) { return this.profileDescriptions[name] || { label: name, desc: '' }; },

    // Step 4: Try It chat
    tryItMessages: [],
    tryItInput: '',
    tryItSending: false,
    get suggestedMessages() {
      var t = window.i18n ? window.i18n.t.bind(window.i18n) : function(k) { return k; };
      return {
        'General': [
          t('wizard.suggestions.general.1') || 'What can you help me with?',
          t('wizard.suggestions.general.2') || 'Tell me a fun fact',
          t('wizard.suggestions.general.3') || 'Summarize the latest AI news'
        ],
        'Development': [
          t('wizard.suggestions.development.1') || 'Write a Python hello world',
          t('wizard.suggestions.development.2') || 'Explain async/await',
          t('wizard.suggestions.development.3') || 'Review this code snippet'
        ],
        'Research': [
          t('wizard.suggestions.research.1') || 'Explain quantum computing simply',
          t('wizard.suggestions.research.2') || 'Compare React vs Vue',
          t('wizard.suggestions.research.3') || 'What are the latest trends in AI?'
        ],
        'Writing': [
          t('wizard.suggestions.writing.1') || 'Help me write a professional email',
          t('wizard.suggestions.writing.2') || 'Improve this paragraph',
          t('wizard.suggestions.writing.3') || 'Write a blog intro about AI'
        ],
        'Business': [
          t('wizard.suggestions.business.1') || 'Draft a meeting agenda',
          t('wizard.suggestions.business.2') || 'How do I handle a complaint?',
          t('wizard.suggestions.business.3') || 'Create a project status update'
        ]
      };
    },
    get currentSuggestions() {
      var tpl = this.templates[this.selectedTemplate];
      var cat = tpl ? tpl.category : 'General';
      return this.suggestedMessages[cat] || this.suggestedMessages['General'];
    },
    async sendTryItMessage(text) {
      if (!text || !text.trim() || !this.createdAgent || this.tryItSending) return;
      text = text.trim();
      this.tryItInput = '';
      this.tryItMessages.push({ role: 'user', text: text });
      this.tryItSending = true;
      try {
        var res = await OpenFangAPI.post('/api/agents/' + this.createdAgent.id + '/message', { message: text });
        this.tryItMessages.push({ role: 'agent', text: res.response || '(no response)' });
        localStorage.setItem('of-first-msg', 'true');
      } catch(e) {
        this.tryItMessages.push({ role: 'agent', text: 'Error: ' + (e.message || 'Could not reach agent') });
      }
      this.tryItSending = false;
    },

    // Step 5: Channel setup (optional)
    channelType: '',
    get channelOptions() {
      var t = window.i18n ? window.i18n.t.bind(window.i18n) : function(k) { return k; };
      return [
        {
          name: 'telegram',
          display_name: t('wizard.channel_telegram') || 'Telegram',
          icon: 'TG',
          description: t('wizard.channel_telegram_desc') || 'Connect your agent to a Telegram bot for messaging.',
          token_label: t('wizard.channel_telegram_token') || 'Bot Token',
          token_placeholder: '123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11',
          token_env: 'TELEGRAM_BOT_TOKEN',
          help: t('wizard.channel_telegram_help') || 'Create a bot via @BotFather on Telegram to get your token.'
        },
        {
          name: 'discord',
          display_name: t('wizard.channel_discord') || 'Discord',
          icon: 'DC',
          description: t('wizard.channel_discord_desc') || 'Connect your agent to a Discord server via bot token.',
          token_label: t('wizard.channel_discord_token') || 'Bot Token',
          token_placeholder: 'MTIz...abc',
          token_env: 'DISCORD_BOT_TOKEN',
          help: t('wizard.channel_discord_help') || 'Create a Discord application at discord.com/developers and add a bot.'
        },
        {
          name: 'slack',
          display_name: t('wizard.channel_slack') || 'Slack',
          icon: 'SL',
          description: t('wizard.channel_slack_desc') || 'Connect your agent to a Slack workspace.',
          token_label: t('wizard.channel_slack_token') || 'Bot Token',
          token_placeholder: 'xoxb-...',
          token_env: 'SLACK_BOT_TOKEN',
          help: t('wizard.channel_slack_help') || 'Create a Slack app at api.slack.com/apps and install it to your workspace.'
        }
      ];
    },
    channelToken: '',
    configuringChannel: false,
    channelConfigured: false,

    // Step 5: Summary
    setupSummary: {
      provider: '',
      agent: '',
      channel: ''
    },

    // ── Lifecycle ──

    async loadData() {
      this.loading = true;
      this.error = '';
      try {
        await this.loadProviders();
        // Pre-select first unconfigured provider, or first one
        var unconfigured = this.providers.filter(function(p) {
          return p.auth_status !== 'configured' && p.api_key_env;
        });
        if (unconfigured.length > 0) {
          this.selectedProvider = unconfigured[0].id;
        } else if (this.providers.length > 0) {
          this.selectedProvider = this.providers[0].id;
        }
      } catch(e) {
        this.error = e.message || 'Could not load setup data.';
      }
      this.loading = false;
    },

    // ── Navigation ──

    nextStep() {
      if (this.step === 3 && !this.createdAgent) {
        // Skip "Try It" if no agent was created
        this.step = 5;
      } else if (this.step < this.totalSteps) {
        this.step++;
      }
    },

    prevStep() {
      if (this.step === 5 && !this.createdAgent) {
        // Skip back past "Try It" if no agent was created
        this.step = 3;
      } else if (this.step > 1) {
        this.step--;
      }
    },

    goToStep(n) {
      if (n >= 1 && n <= this.totalSteps) {
        if (n === 4 && !this.createdAgent) return; // Can't go to Try It without agent
        this.step = n;
      }
    },

    stepLabel(n) {
      var t = window.i18n ? window.i18n.t.bind(window.i18n) : function(k) { return k; };
      var labels = [
        t('wizard.step_welcome') || 'Welcome',
        t('wizard.step_provider') || 'Provider',
        t('wizard.step_agent') || 'Agent',
        t('wizard.step_try_it') || 'Try It',
        t('wizard.step_channel') || 'Channel',
        t('wizard.step_done') || 'Done'
      ];
      return labels[n - 1] || '';
    },

    get canGoNext() {
      if (this.step === 2) return this.keySaved || this.hasConfiguredProvider || this.claudeCodeDetected;
      if (this.step === 3) return this.agentName.trim().length > 0;
      return true;
    },

    claudeCodeDetected: false,

    get hasConfiguredProvider() {
      var self = this;
      return this.providers.some(function(p) {
        return p.auth_status === 'configured';
      });
    },

    // ── Step 2: Providers ──

    async loadProviders() {
      try {
        var data = await OpenFangAPI.get('/api/providers');
        this.providers = data.providers || [];
      } catch(e) { this.providers = []; }
    },

    get selectedProviderObj() {
      var self = this;
      var match = this.providers.filter(function(p) { return p.id === self.selectedProvider; });
      return match.length > 0 ? match[0] : null;
    },

    get popularProviders() {
      var popular = ['anthropic', 'openai', 'gemini', 'groq', 'deepseek', 'openrouter', 'claude-code'];
      return this.providers.filter(function(p) {
        return popular.indexOf(p.id) >= 0;
      }).sort(function(a, b) {
        return popular.indexOf(a.id) - popular.indexOf(b.id);
      });
    },

    get otherProviders() {
      var popular = ['anthropic', 'openai', 'gemini', 'groq', 'deepseek', 'openrouter', 'claude-code'];
      return this.providers.filter(function(p) {
        return popular.indexOf(p.id) < 0;
      });
    },

    selectProvider(id) {
      this.selectedProvider = id;
      this.apiKeyInput = '';
      this.testResult = null;
      this.keySaved = false;
    },

    providerHelp: function(id) {
      var help = {
        anthropic: { url: 'https://console.anthropic.com/settings/keys', text: 'Get your key from the Anthropic Console' },
        openai: { url: 'https://platform.openai.com/api-keys', text: 'Get your key from the OpenAI Platform' },
        gemini: { url: 'https://aistudio.google.com/apikey', text: 'Get your key from Google AI Studio' },
        groq: { url: 'https://console.groq.com/keys', text: 'Get your key from the Groq Console (free tier available)' },
        deepseek: { url: 'https://platform.deepseek.com/api_keys', text: 'Get your key from the DeepSeek Platform (very affordable)' },
        openrouter: { url: 'https://openrouter.ai/keys', text: 'Get your key from OpenRouter (access 100+ models with one key)' },
        mistral: { url: 'https://console.mistral.ai/api-keys', text: 'Get your key from the Mistral Console' },
        together: { url: 'https://api.together.xyz/settings/api-keys', text: 'Get your key from Together AI' },
        fireworks: { url: 'https://fireworks.ai/account/api-keys', text: 'Get your key from Fireworks AI' },
        perplexity: { url: 'https://www.perplexity.ai/settings/api', text: 'Get your key from Perplexity Settings' },
        cohere: { url: 'https://dashboard.cohere.com/api-keys', text: 'Get your key from the Cohere Dashboard' },
        xai: { url: 'https://console.x.ai/', text: 'Get your key from the xAI Console' },
        'claude-code': { url: 'https://docs.anthropic.com/en/docs/claude-code', text: 'Install: npm install -g @anthropic-ai/claude-code && claude auth (no API key needed)' }
      };
      return help[id] || null;
    },

    providerIsConfigured(p) {
      return p && p.auth_status === 'configured';
    },

    async saveKey() {
      var provider = this.selectedProviderObj;
      if (!provider) return;
      var key = this.apiKeyInput.trim();
      if (!key) {
        OpenFangToast.error(window.i18n ? window.i18n.t('wizard.enter_api_key') : 'Please enter an API key');
        return;
      }
      this.savingKey = true;
      try {
        await OpenFangAPI.post('/api/providers/' + encodeURIComponent(provider.id) + '/key', { key: key });
        this.apiKeyInput = '';
        this.keySaved = true;
        this.setupSummary.provider = provider.display_name;
        OpenFangToast.success((window.i18n ? window.i18n.t('wizard.api_key_saved') : 'API key saved for') + ' ' + provider.display_name);
        await this.loadProviders();
        // Auto-test after saving
        await this.testKey();
      } catch(e) {
        OpenFangToast.error((window.i18n ? window.i18n.t('wizard.failed_save_key') : 'Failed to save key:') + ' ' + e.message);
      }
      this.savingKey = false;
    },

    async testKey() {
      var provider = this.selectedProviderObj;
      if (!provider) return;
      this.testingProvider = true;
      this.testResult = null;
      try {
        var result = await OpenFangAPI.post('/api/providers/' + encodeURIComponent(provider.id) + '/test', {});
        this.testResult = result;
        if (result.status === 'ok') {
          OpenFangToast.success(provider.display_name + ' ' + (window.i18n ? window.i18n.t('wizard.connected') : 'connected') + ' (' + (result.latency_ms || '?') + 'ms)');
        } else {
          OpenFangToast.error(provider.display_name + ': ' + (result.error || (window.i18n ? window.i18n.t('wizard.connection_failed') : 'Connection failed')));
        }
      } catch(e) {
        this.testResult = { status: 'error', error: e.message };
        OpenFangToast.error((window.i18n ? window.i18n.t('wizard.test_failed') : 'Test failed:') + ' ' + e.message);
      }
      this.testingProvider = false;
    },

    async detectClaudeCode() {
      this.testingProvider = true;
      this.testResult = null;
      try {
        var result = await OpenFangAPI.post('/api/providers/claude-code/test', {});
        this.testResult = result;
        if (result.status === 'ok') {
          this.claudeCodeDetected = true;
          this.keySaved = true;
          this.setupSummary.provider = 'Claude Code';
          OpenFangToast.success('Claude Code detected (' + (result.latency_ms || '?') + 'ms)');
        } else {
          this.testResult = { status: 'error', error: 'Claude Code CLI not detected' };
          OpenFangToast.error('Claude Code CLI not detected. Make sure you\'ve run: npm install -g @anthropic-ai/claude-code && claude auth');
        }
      } catch(e) {
        this.testResult = { status: 'error', error: e.message };
        OpenFangToast.error('Claude Code CLI not detected. Make sure you\'ve run: npm install -g @anthropic-ai/claude-code && claude auth');
      }
      this.testingProvider = false;
    },

    // ── Step 3: Agent creation ──

    selectTemplate(index) {
      this.selectedTemplate = index;
      var tpl = this.templates[index];
      if (tpl) {
        this.agentName = tpl.name.toLowerCase().replace(/\s+/g, '-');
      }
    },

    async createAgent() {
      var tpl = this.templates[this.selectedTemplate];
      if (!tpl) return;
      var name = this.agentName.trim();
      if (!name) {
        OpenFangToast.error(window.i18n ? window.i18n.t('wizard.enter_agent_name') : 'Please enter a name for your agent');
        return;
      }

      // Use the provider the user just configured, or the template default
      var provider = tpl.provider;
      var model = tpl.model;
      if (this.selectedProviderObj && this.providerIsConfigured(this.selectedProviderObj)) {
        provider = this.selectedProviderObj.id;
        // Use a sensible default model for the provider
        model = this.defaultModelForProvider(provider) || tpl.model;
      }

      var toml = '[agent]\n';
      toml += 'name = "' + wizardTomlBasicEscape(name) + '"\n';
      toml += 'description = "' + wizardTomlBasicEscape(tpl.description) + '"\n';
      toml += 'profile = "' + tpl.profile + '"\n\n';
      toml += '[model]\nprovider = "' + provider + '"\n';
      toml += 'model = "' + model + '"\n';
      toml += 'system_prompt = """\n' + wizardTomlMultilineEscape(tpl.system_prompt) + '\n"""\n';

      this.creatingAgent = true;
      try {
        var res = await OpenFangAPI.post('/api/agents', { manifest_toml: toml });
        if (res.agent_id) {
          this.createdAgent = { id: res.agent_id, name: res.name || name };
          this.setupSummary.agent = res.name || name;
          OpenFangToast.success((window.i18n ? window.i18n.t('wizard.agent_created') : 'Agent') + ' "' + (res.name || name) + '" ' + (window.i18n ? window.i18n.t('wizard.agent_created_suffix') || 'created' : 'created'));
          await Alpine.store('app').refreshAgents();
        } else {
          OpenFangToast.error((window.i18n ? window.i18n.t('wizard.failed_create_agent') : 'Failed:') + ' ' + (res.error || 'Unknown error'));
        }
      } catch(e) {
        OpenFangToast.error((window.i18n ? window.i18n.t('wizard.failed_create_agent') : 'Failed to create agent:') + ' ' + e.message);
      }
      this.creatingAgent = false;
    },

    defaultModelForProvider(providerId) {
      var defaults = {
        anthropic: 'claude-sonnet-4-20250514',
        openai: 'gpt-4o',
        gemini: 'gemini-2.5-flash',
        groq: 'llama-3.3-70b-versatile',
        deepseek: 'deepseek-chat',
        openrouter: 'openrouter/google/gemini-2.5-flash',
        mistral: 'mistral-large-latest',
        together: 'meta-llama/Llama-3-70b-chat-hf',
        fireworks: 'accounts/fireworks/models/llama-v3p1-70b-instruct',
        perplexity: 'llama-3.1-sonar-large-128k-online',
        cohere: 'command-r-plus',
        xai: 'grok-2',
        'claude-code': 'claude-code/sonnet'
      };
      return defaults[providerId] || '';
    },

    // ── Step 5: Channel setup ──

    selectChannel(name) {
      if (this.channelType === name) {
        this.channelType = '';
        this.channelToken = '';
      } else {
        this.channelType = name;
        this.channelToken = '';
      }
    },

    get selectedChannelObj() {
      var self = this;
      var match = this.channelOptions.filter(function(ch) { return ch.name === self.channelType; });
      return match.length > 0 ? match[0] : null;
    },

    async configureChannel() {
      var ch = this.selectedChannelObj;
      if (!ch) return;
      var token = this.channelToken.trim();
      if (!token) {
        OpenFangToast.error((window.i18n ? window.i18n.t('wizard.enter_token') : 'Please enter the') + ' ' + ch.token_label);
        return;
      }
      this.configuringChannel = true;
      try {
        var fields = {};
        fields[ch.token_env.toLowerCase()] = token;
        fields.token = token;
        await OpenFangAPI.post('/api/channels/' + ch.name + '/configure', { fields: fields });
        this.channelConfigured = true;
        this.setupSummary.channel = ch.display_name;
        OpenFangToast.success(ch.display_name + ' ' + (window.i18n ? window.i18n.t('wizard.channel_configured') : 'configured and activated.'));
      } catch(e) {
        OpenFangToast.error((window.i18n ? window.i18n.t('wizard.failed_configure') : 'Failed:') + ' ' + (e.message || 'Unknown error'));
      }
      this.configuringChannel = false;
    },

    // ── Step 6: Finish ──

    finish() {
      localStorage.setItem('openfang-onboarded', 'true');
      Alpine.store('app').showOnboarding = false;
      // Navigate to agents with chat if an agent was created, otherwise overview
      if (this.createdAgent) {
        var agent = this.createdAgent;
        Alpine.store('app').pendingAgent = { id: agent.id, name: agent.name, model_provider: '?', model_name: '?' };
        window.location.hash = 'agents';
      } else {
        window.location.hash = 'overview';
      }
    },

    finishAndDismiss() {
      localStorage.setItem('openfang-onboarded', 'true');
      Alpine.store('app').showOnboarding = false;
      window.location.hash = 'overview';
    }
  };
}
