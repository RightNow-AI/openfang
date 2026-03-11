// Operis — Nền tảng vận hành doanh nghiệp thông minh
// Giao diện quản lý agent & công ty
'use strict';

function agentsPage() {
  return {
    tab: 'agents',
    activeChatAgent: null,
    // -- Agents state --
    showSpawnModal: false,
    showDetailModal: false,
    detailAgent: null,
    spawnMode: 'quick',
    spawning: false,
    spawnToml: '',
    filterState: 'all',
    loading: true,
    loadError: '',
    spawnForm: {
      name: '',
      provider: 'openai',
      model: 'kimi-k2-250905',
      systemPrompt: '',
      profile: 'full',
      caps: { memory_read: true, memory_write: true, network: false, shell: true, agent_spawn: false }
    },

    // -- Trạng thái tạo công ty --
    showCompanyModal: false,
    companyStep: 1,
    companyForm: {
      name: '', tax_id: '', industry: '', address: '', representative: '', phone: '', email: ''
    },

    // -- Templates cho doanh nghiệp --
    selectedCategory: 'Tất cả',
    searchQuery: '',

    builtinTemplates: [
      {
        name: 'Trợ lý Giám đốc',
        description: 'Báo cáo tổng quan, duyệt đơn hàng vượt quyền, theo dõi hiệu suất toàn công ty.',
        category: 'Quản lý',
        icon: '👔',
        provider: 'openai',
        model: 'kimi-k2-250905',
        profile: 'full',
        system_prompt: 'Bạn là trợ lý AI cho Giám đốc. Luôn dùng tiếng Việt. Báo cáo ngắn gọn bằng số liệu. Ưu tiên: escalation chờ duyệt, dòng tiền, hiệu suất nhân viên. Khi nhận lệnh, chạy report:director để lấy tổng quan.'
      },
      {
        name: 'Trợ lý Kinh doanh',
        description: 'Chốt đơn, báo giá, quản lý khách hàng, theo dõi hợp đồng và doanh thu.',
        category: 'Kinh doanh',
        icon: '💼',
        provider: 'openai',
        model: 'kimi-k2-250905',
        profile: 'full',
        system_prompt: 'Bạn là trợ lý AI cho bộ phận Kinh doanh. Luôn dùng tiếng Việt. Hỗ trợ sale chốt đơn, báo giá, tạo hợp đồng. Luôn check auth:check trước khi quyết định. Nếu vượt quyền → escalate lên cấp trên.'
      },
      {
        name: 'Trợ lý Kế toán',
        description: 'Ghi nhận thu chi, quản lý công nợ, báo cáo tài chính, theo dõi thanh toán hợp đồng.',
        category: 'Tài chính',
        icon: '📊',
        provider: 'openai',
        model: 'kimi-k2-250905',
        profile: 'full',
        system_prompt: 'Bạn là trợ lý AI cho Kế toán. Luôn dùng tiếng Việt. Số tiền luôn dạng INTEGER đồng Việt Nam. Hỗ trợ ghi nhận thanh toán, báo cáo dòng tiền, công nợ phải thu/phải trả. Chạy report:cashflow, report:receivables khi được hỏi.'
      },
      {
        name: 'Trợ lý Sản xuất',
        description: 'Quản lý xưởng, phân công thợ, theo dõi vật tư, tiến độ sản xuất.',
        category: 'Vận hành',
        icon: '🏭',
        provider: 'openai',
        model: 'kimi-k2-250905',
        profile: 'full',
        system_prompt: 'Bạn là trợ lý AI cho bộ phận Sản xuất. Luôn dùng tiếng Việt. Hỗ trợ quản lý xưởng, đặt vật tư, phân công công việc. Check auth:check trước khi duyệt chi. Nếu vượt hạn mức → escalate.'
      },
      {
        name: 'Onboard công ty mới',
        description: 'Thu thập thông tin công ty, nhân sự, quy trình → tự động tạo hệ thống hoàn chỉnh.',
        category: 'Thiết lập',
        icon: '🏢',
        provider: 'openai',
        model: 'kimi-k2-250905',
        profile: 'full',
        system_prompt: `Bạn là chuyên gia onboard doanh nghiệp cho Operis. LUÔN dùng tiếng Việt. Phong cách: chuyên nghiệp, thân thiện, hỏi từng bước rõ ràng.

## OPERIS LÀ GÌ?
Operis là **hệ thống trung gian điều phối công việc** trong doanh nghiệp. Cách hoạt động:
- **Mỗi nhân viên** sẽ chat trực tiếp với AI (qua Telegram, web, hoặc app)
- AI **biết workflow, SOP, thẩm quyền** của từng người → tự động hướng dẫn, nhắc việc, kiểm tra quyền hạn
- Khi một bước trong workflow hoàn thành → AI **tự động chuyển việc đến người tiếp theo** theo luồng
- Ví dụ: Sale tiếp nhận khách → AI tự báo kế toán → kế toán tạo báo giá → AI gửi lên GĐ duyệt → duyệt xong AI thông báo Sale
- AI cũng **giám sát workflow**: phát hiện công việc bị treo, nhắc nhở, escalate khi cần
- Giám đốc nhìn tổng quan: ai đang làm gì, workflow nào đang tắc, tài chính ra sao

Tóm lại: Operis = **AI điều phối viên** đứng giữa tất cả nhân viên, biết mọi quy trình, tự động giao-nhận-nhắc-duyệt công việc.

Khi giải thích cho khách hàng, hãy nói rõ: "Hệ thống sẽ là trung gian để điều phối công việc giữa các nhân viên. Mỗi nhân viên sẽ chat trực tiếp với AI, AI sẽ theo workflow để chuyển việc đến đúng người, đúng lúc."

## MỤC TIÊU
Thu thập đủ thông tin để chạy lệnh company:setup tạo toàn bộ hệ thống quản lý doanh nghiệp.

## QUY TRÌNH HỎI (6 bước)

### Bước 1: Thông tin công ty
Hỏi lần lượt:
- Tên công ty đầy đủ
- Mã số thuế (MST)
- Ngành nghề chính (xây dựng, nội thất, thương mại, dịch vụ, sản xuất, F&B, v.v.)
- Địa chỉ
- Người đại diện pháp luật
- SĐT & email công ty
- Ngân hàng & số tài khoản (nếu có)

### Bước 2: Cơ cấu tổ chức
- Công ty có những phòng ban nào?
- Mỗi phòng ban có bao nhiêu người?
- Liệt kê nhân viên chủ chốt: Tên, Mã NV (hoặc để trống sẽ tự sinh), Chức vụ, Phòng ban, SĐT
- Ai là giám đốc? Ai báo cáo cho ai?

### Bước 3: Thẩm quyền & hạn mức
- Giám đốc duyệt tất cả? Hay có phó GĐ?
- Sale tự chốt đơn được bao nhiêu tiền? (mặc định 50 triệu)
- Trưởng phòng duyệt chi bao nhiêu? (mặc định 20 triệu)
- Nhân viên thường chi tiêu tối đa bao nhiêu? (mặc định 2 triệu)

Nếu khách nói "mặc định" hoặc "tùy bạn" → dùng giá trị mặc định, KHÔNG hỏi lại.

### Bước 4: Quy trình làm việc
- Quy trình bán hàng của công ty như thế nào? (từ tiếp nhận KH đến thu tiền)
- Có quy trình duyệt chi không?
- Có quy trình đặc thù ngành không? (thi công, sản xuất, giao hàng...)

Nếu khách không mô tả chi tiết → hệ thống sẽ TỰ SINH workflow dựa trên ngành nghề.

### Bước 5: Yêu cầu đặc biệt
Hỏi: "Anh/chị có cần quản lý thêm module nào không? Ví dụ:"
- Quản lý kho (nhập/xuất/tồn kho)
- Quản lý khách hàng CRM
- Chấm công
- Quản lý dự án
- Hoặc module khác?

Nếu CÓ → tạo custom_modules trong JSON. Ví dụ kho:
{
  "code": "inventory",
  "name": "Quản lý kho",
  "icon": "📦",
  "fields": [
    {"name": "product_name", "type": "text", "required": true},
    {"name": "sku", "type": "text", "required": false},
    {"name": "quantity", "type": "number", "required": true},
    {"name": "unit", "type": "text", "required": true},
    {"name": "unit_price", "type": "number", "required": false},
    {"name": "warehouse", "type": "text", "required": false}
  ]
}
Đồng thời tạo SOP tương ứng cho module đó.

### Bước 6: Xác nhận & Thực thi
Khi ĐỦ thông tin:
1. Tóm tắt lại toàn bộ dưới dạng bảng đẹp:
   - Công ty: tên, MST, ngành
   - Phòng ban: danh sách
   - Nhân viên: bảng (mã, tên, chức vụ, phòng ban)
   - Thẩm quyền: tóm tắt hạn mức
   - Workflow: danh sách quy trình
   - Module thêm: nếu có
2. Hỏi: "Thông tin đã đầy đủ chưa? Tôi sẽ tạo hệ thống ngay."
3. Khi khách xác nhận → Ghi JSON ra file rồi gọi:
   node "C:/Users/Admin/Desktop/test operis/ofn-finance/src/cli.js" <tenant_id> company:setup <file.json>
4. Hiển thị kết quả: đã tạo bao nhiêu phòng ban, nhân viên, SOP, workflow, v.v.

## QUY TẮC QUAN TRỌNG
- KHÔNG hỏi tất cả cùng lúc. Hỏi từng bước, chờ trả lời.
- Nếu khách trả lời ngắn gọn → tự suy luận hợp lý, đề xuất và xác nhận.
- Nếu khách nói "thêm module X" → tạo custom_module với fields phù hợp + SOP tương ứng.
- Nếu thiếu thông tin không quan trọng (email, MST) → bỏ qua, tạo sau.
- Mã nhân viên: nếu không cung cấp → tự sinh (KD001, KT001, TC001, GD001...).
- Tiền VND: luôn dùng INTEGER. 50 triệu = 50000000.
- tenant_id = tên công ty viết thường, thay dấu cách bằng _, bỏ dấu tiếng Việt.

## VÍ DỤ TENANT_ID
- "Nội Thất Hưng Thịnh" → noi_that_hung_thinh
- "Xây Dựng ABC" → xay_dung_abc
- "Công ty TNHH Minh Phát" → minh_phat`
      }
    ],

    // -- Detail modal --
    detailTab: 'info',
    agentFiles: [],
    editingFile: null,
    fileContent: '',
    fileSaving: false,
    filesLoading: false,
    configForm: {},
    configSaving: false,

    // -- Templates state --
    tplTemplates: [],
    tplProviders: [],
    tplLoading: false,
    tplLoadError: '',

    // ── Profile Descriptions ──
    profileDescriptions: {
      minimal: { label: 'Tối thiểu', desc: 'Chỉ đọc file' },
      coding: { label: 'Lập trình', desc: 'File + shell + web' },
      research: { label: 'Nghiên cứu', desc: 'Tìm kiếm web + đọc/ghi file' },
      messaging: { label: 'Nhắn tin', desc: 'Giao tiếp agent + bộ nhớ' },
      automation: { label: 'Tự động', desc: 'Tất cả công cụ trừ tùy chỉnh' },
      full: { label: 'Đầy đủ', desc: 'Tất cả 35+ công cụ' }
    },
    profileInfo: function(name) {
      return this.profileDescriptions[name] || { label: name, desc: '' };
    },

    // ── Tool Preview ──
    spawnProfiles: [],
    spawnProfilesLoaded: false,
    async loadSpawnProfiles() {
      if (this.spawnProfilesLoaded) return;
      try {
        var data = await OperisAPI.get('/api/profiles');
        this.spawnProfiles = data.profiles || [];
        this.spawnProfilesLoaded = true;
      } catch(e) { this.spawnProfiles = []; }
    },
    get selectedProfileTools() {
      var pname = this.spawnForm.profile;
      var match = this.spawnProfiles.find(function(p) { return p.name === pname; });
      if (match && match.tools) return match.tools.slice(0, 15);
      return [];
    },

    get agents() { return Alpine.store('app').agents; },

    get filteredAgents() {
      var f = this.filterState;
      if (f === 'all') return this.agents;
      return this.agents.filter(function(a) { return a.state.toLowerCase() === f; });
    },

    get runningCount() {
      return this.agents.filter(function(a) { return a.state === 'Running'; }).length;
    },

    get stoppedCount() {
      return this.agents.filter(function(a) { return a.state !== 'Running'; }).length;
    },

    // -- Template filters --
    get categories() {
      var cats = { 'Tất cả': true };
      this.builtinTemplates.forEach(function(t) { cats[t.category] = true; });
      return Object.keys(cats);
    },

    get filteredBuiltins() {
      var self = this;
      return this.builtinTemplates.filter(function(t) {
        if (self.selectedCategory !== 'Tất cả' && t.category !== self.selectedCategory) return false;
        if (self.searchQuery) {
          var q = self.searchQuery.toLowerCase();
          if (t.name.toLowerCase().indexOf(q) === -1 &&
              t.description.toLowerCase().indexOf(q) === -1) return false;
        }
        return true;
      });
    },

    get filteredCustom() {
      var self = this;
      return this.tplTemplates.filter(function(t) {
        if (self.searchQuery) {
          var q = self.searchQuery.toLowerCase();
          if ((t.name || '').toLowerCase().indexOf(q) === -1 &&
              (t.description || '').toLowerCase().indexOf(q) === -1) return false;
        }
        return true;
      });
    },

    isProviderConfigured(providerName) {
      if (!providerName) return false;
      var p = this.tplProviders.find(function(pr) { return pr.id === providerName; });
      return p ? p.auth_status === 'configured' : false;
    },

    async init() {
      var self = this;
      this.loading = true;
      this.loadError = '';
      try {
        await Alpine.store('app').refreshAgents();
      } catch(e) {
        this.loadError = e.message || 'Không thể tải danh sách agent. Daemon có đang chạy không?';
      }
      this.loading = false;

      var store = Alpine.store('app');
      if (store.pendingAgent) {
        this.activeChatAgent = store.pendingAgent;
      }
      this.$watch('$store.app.pendingAgent', function(agent) {
        if (agent) {
          self.activeChatAgent = agent;
        }
      });
    },

    async loadData() {
      this.loading = true;
      this.loadError = '';
      try {
        await Alpine.store('app').refreshAgents();
      } catch(e) {
        this.loadError = e.message || 'Không thể tải danh sách agent.';
      }
      this.loading = false;
    },

    async loadTemplates() {
      this.tplLoading = true;
      this.tplLoadError = '';
      try {
        var results = await Promise.all([
          OperisAPI.get('/api/templates'),
          OperisAPI.get('/api/providers').catch(function() { return { providers: [] }; })
        ]);
        this.tplTemplates = results[0].templates || [];
        this.tplProviders = results[1].providers || [];
      } catch(e) {
        this.tplTemplates = [];
        this.tplLoadError = e.message || 'Không thể tải templates.';
      }
      this.tplLoading = false;
    },

    chatWithAgent(agent) {
      Alpine.store('app').pendingAgent = agent;
      this.activeChatAgent = agent;
    },

    closeChat() {
      this.activeChatAgent = null;
      OperisAPI.wsDisconnect();
    },

    showDetail(agent) {
      this.detailAgent = agent;
      this.detailTab = 'info';
      this.agentFiles = [];
      this.editingFile = null;
      this.fileContent = '';
      this.configForm = {
        name: agent.name || '',
        system_prompt: agent.system_prompt || '',
        emoji: (agent.identity && agent.identity.emoji) || '',
        color: (agent.identity && agent.identity.color) || '#FF5C00',
        archetype: (agent.identity && agent.identity.archetype) || '',
        vibe: (agent.identity && agent.identity.vibe) || ''
      };
      this.showDetailModal = true;
    },

    killAgent(agent) {
      var self = this;
      OperisToast.confirm('Dừng Agent', 'Dừng agent "' + agent.name + '"?', async function() {
        try {
          await OperisAPI.del('/api/agents/' + agent.id);
          OperisToast.success('Đã dừng "' + agent.name + '"');
          self.showDetailModal = false;
          await Alpine.store('app').refreshAgents();
        } catch(e) {
          OperisToast.error('Lỗi dừng agent: ' + e.message);
        }
      });
    },

    killAllAgents() {
      var list = this.filteredAgents;
      if (!list.length) return;
      OperisToast.confirm('Dừng tất cả', 'Dừng ' + list.length + ' agent?', async function() {
        var errors = [];
        for (var i = 0; i < list.length; i++) {
          try {
            await OperisAPI.del('/api/agents/' + list[i].id);
          } catch(e) { errors.push(list[i].name + ': ' + e.message); }
        }
        await Alpine.store('app').refreshAgents();
        if (errors.length) {
          OperisToast.error('Một số agent lỗi: ' + errors.join(', '));
        } else {
          OperisToast.success('Đã dừng ' + list.length + ' agent');
        }
      });
    },

    // ── Tạo agent nhanh ──
    openSpawnWizard() {
      this.showSpawnModal = true;
      this.spawnMode = 'quick';
      this.spawnForm.name = '';
      this.spawnForm.systemPrompt = 'Bạn là trợ lý AI cho doanh nghiệp. Luôn dùng tiếng Việt. Hỗ trợ nhân viên theo quy trình SOP, kiểm tra thẩm quyền trước khi quyết định.';
      this.spawnForm.profile = 'full';
    },

    generateToml() {
      var f = this.spawnForm;
      var lines = [
        'name = "' + f.name + '"',
        'module = "builtin:chat"'
      ];
      if (f.profile && f.profile !== 'custom') {
        lines.push('profile = "' + f.profile + '"');
      }
      lines.push('', '[model]');
      lines.push('provider = "' + f.provider + '"');
      lines.push('model = "' + f.model + '"');
      lines.push('system_prompt = "' + f.systemPrompt.replace(/"/g, '\\"') + '"');
      return lines.join('\n');
    },

    async setMode(agent, mode) {
      try {
        await OperisAPI.put('/api/agents/' + agent.id + '/mode', { mode: mode });
        agent.mode = mode;
        OperisToast.success('Chế độ: ' + mode);
        await Alpine.store('app').refreshAgents();
      } catch(e) {
        OperisToast.error('Lỗi đặt chế độ: ' + e.message);
      }
    },

    async spawnAgent() {
      this.spawning = true;
      var toml = this.spawnMode === 'quick' ? this.generateToml() : this.spawnToml;
      if (!toml.trim()) {
        this.spawning = false;
        OperisToast.warn('Chưa có cấu hình agent');
        return;
      }

      try {
        var res = await OperisAPI.post('/api/agents', { manifest_toml: toml });
        if (res.agent_id) {
          this.showSpawnModal = false;
          this.spawnForm.name = '';
          this.spawnToml = '';
          OperisToast.success('Đã tạo "' + (res.name || 'mới') + '"');
          await Alpine.store('app').refreshAgents();
          this.chatWithAgent({ id: res.agent_id, name: res.name, model_provider: '?', model_name: '?' });
        } else {
          OperisToast.error('Tạo thất bại: ' + (res.error || 'Lỗi không xác định'));
        }
      } catch(e) {
        OperisToast.error('Lỗi tạo agent: ' + e.message);
      }
      this.spawning = false;
    },

    // ── Detail modal: Files tab ──
    async loadAgentFiles() {
      if (!this.detailAgent) return;
      this.filesLoading = true;
      try {
        var data = await OperisAPI.get('/api/agents/' + this.detailAgent.id + '/files');
        this.agentFiles = data.files || [];
      } catch(e) {
        this.agentFiles = [];
        OperisToast.error('Lỗi tải file: ' + e.message);
      }
      this.filesLoading = false;
    },

    async openFile(file) {
      if (!file.exists) {
        this.editingFile = file.name;
        this.fileContent = '';
        return;
      }
      try {
        var data = await OperisAPI.get('/api/agents/' + this.detailAgent.id + '/files/' + encodeURIComponent(file.name));
        this.editingFile = file.name;
        this.fileContent = data.content || '';
      } catch(e) {
        OperisToast.error('Lỗi đọc file: ' + e.message);
      }
    },

    async saveFile() {
      if (!this.editingFile || !this.detailAgent) return;
      this.fileSaving = true;
      try {
        await OperisAPI.put('/api/agents/' + this.detailAgent.id + '/files/' + encodeURIComponent(this.editingFile), { content: this.fileContent });
        OperisToast.success('Đã lưu ' + this.editingFile);
        await this.loadAgentFiles();
      } catch(e) {
        OperisToast.error('Lỗi lưu file: ' + e.message);
      }
      this.fileSaving = false;
    },

    closeFileEditor() {
      this.editingFile = null;
      this.fileContent = '';
    },

    // ── Detail modal: Config tab ──
    async saveConfig() {
      if (!this.detailAgent) return;
      this.configSaving = true;
      try {
        await OperisAPI.patch('/api/agents/' + this.detailAgent.id + '/config', this.configForm);
        OperisToast.success('Đã lưu cấu hình');
        await Alpine.store('app').refreshAgents();
      } catch(e) {
        OperisToast.error('Lỗi lưu cấu hình: ' + e.message);
      }
      this.configSaving = false;
    },

    // ── Clone agent ──
    async cloneAgent(agent) {
      var newName = (agent.name || 'agent') + '-copy';
      try {
        var res = await OperisAPI.post('/api/agents/' + agent.id + '/clone', { new_name: newName });
        if (res.agent_id) {
          OperisToast.success('Đã nhân bản "' + res.name + '"');
          await Alpine.store('app').refreshAgents();
          this.showDetailModal = false;
        }
      } catch(e) {
        OperisToast.error('Lỗi nhân bản: ' + e.message);
      }
    },

    // -- Template methods --
    async spawnFromTemplate(name) {
      try {
        var data = await OperisAPI.get('/api/templates/' + encodeURIComponent(name));
        if (data.manifest_toml) {
          var res = await OperisAPI.post('/api/agents', { manifest_toml: data.manifest_toml });
          if (res.agent_id) {
            OperisToast.success('Đã tạo "' + (res.name || name) + '"');
            await Alpine.store('app').refreshAgents();
            this.chatWithAgent({ id: res.agent_id, name: res.name || name, model_provider: '?', model_name: '?' });
          }
        }
      } catch(e) {
        OperisToast.error('Lỗi tạo từ template: ' + e.message);
      }
    },

    async spawnBuiltin(t) {
      var toml = 'name = "' + t.name + '"\n';
      toml += 'description = "' + t.description.replace(/"/g, '\\"') + '"\n';
      toml += 'module = "builtin:chat"\n';
      toml += 'profile = "' + t.profile + '"\n\n';
      toml += '[model]\nprovider = "' + t.provider + '"\nmodel = "' + t.model + '"\n';
      toml += 'system_prompt = """\n' + t.system_prompt + '\n"""\n';

      try {
        var res = await OperisAPI.post('/api/agents', { manifest_toml: toml });
        if (res.agent_id) {
          OperisToast.success('Đã tạo "' + t.name + '"');
          await Alpine.store('app').refreshAgents();
          this.chatWithAgent({ id: res.agent_id, name: t.name, model_provider: t.provider, model_name: t.model });
        }
      } catch(e) {
        OperisToast.error('Lỗi tạo agent: ' + e.message);
      }
    }
  };
}
