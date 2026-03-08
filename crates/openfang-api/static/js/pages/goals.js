// OpenFang Goals Page — hierarchical goal tracking
'use strict';

function goalsPage() {
  return {
    tab: 'tree',
    goals: [],
    loading: true,
    loadError: '',

    // Create form
    showCreateForm: false,
    newGoal: {
      title: '',
      description: '',
      level: 'task',
      status: 'planned',
      parent_id: '',
      owner_agent_id: '',
      progress: 0
    },
    creating: false,

    // Edit
    editGoal: null,
    editForm: {},
    saving: false,

    // ── Lifecycle ──

    async loadData() {
      this.loading = true;
      this.loadError = '';
      try {
        var data = await OpenFangAPI.get('/api/goals');
        this.goals = data.goals || [];
      } catch(e) {
        this.goals = [];
        this.loadError = e.message || 'Could not load goals.';
      }
      this.loading = false;
    },

    // ── Tree helpers ──

    rootGoals() {
      return this.goals.filter(function(g) { return !g.parent_id; });
    },

    childrenOf(parentId) {
      return this.goals.filter(function(g) { return g.parent_id === parentId; });
    },

    // Build flattened tree order for rendering
    treeOrder() {
      var result = [];
      var self = this;
      function walk(parentId, depth) {
        var children = self.goals.filter(function(g) {
          return parentId ? g.parent_id === parentId : !g.parent_id;
        });
        var levelOrder = { mission: 0, strategy: 1, objective: 2, task: 3 };
        children.sort(function(a, b) {
          var la = levelOrder[a.level] || 4;
          var lb = levelOrder[b.level] || 4;
          if (la !== lb) return la - lb;
          return a.title.localeCompare(b.title);
        });
        for (var i = 0; i < children.length; i++) {
          result.push({ goal: children[i], depth: depth });
          walk(children[i].id, depth + 1);
        }
      }
      walk(null, 0);
      return result;
    },

    // ── Board helpers (kanban) ──

    goalsByStatus(status) {
      return this.goals.filter(function(g) { return g.status === status; });
    },

    // ── CRUD ──

    async createGoal() {
      if (!this.newGoal.title.trim()) {
        OpenFangToast.warn('Please enter a goal title');
        return;
      }
      this.creating = true;
      try {
        var body = {
          title: this.newGoal.title,
          description: this.newGoal.description || null,
          level: this.newGoal.level,
          status: this.newGoal.status,
          parent_id: this.newGoal.parent_id || null,
          owner_agent_id: this.newGoal.owner_agent_id || null,
          progress: parseInt(this.newGoal.progress, 10) || 0
        };
        await OpenFangAPI.post('/api/goals', body);
        this.showCreateForm = false;
        this.newGoal = { title: '', description: '', level: 'task', status: 'planned', parent_id: '', owner_agent_id: '', progress: 0 };
        OpenFangToast.success('Goal created');
        await this.loadData();
      } catch(e) {
        OpenFangToast.error('Failed to create goal: ' + (e.message || e));
      }
      this.creating = false;
    },

    openEdit(goal) {
      this.editGoal = goal;
      this.editForm = {
        title: goal.title,
        description: goal.description || '',
        level: goal.level,
        status: goal.status,
        parent_id: goal.parent_id || '',
        owner_agent_id: goal.owner_agent_id || '',
        progress: goal.progress
      };
    },

    async saveEdit() {
      if (!this.editGoal) return;
      this.saving = true;
      try {
        var body = {};
        if (this.editForm.title !== this.editGoal.title) body.title = this.editForm.title;
        if (this.editForm.description !== (this.editGoal.description || '')) body.description = this.editForm.description || null;
        if (this.editForm.level !== this.editGoal.level) body.level = this.editForm.level;
        if (this.editForm.status !== this.editGoal.status) body.status = this.editForm.status;
        if (this.editForm.parent_id !== (this.editGoal.parent_id || '')) body.parent_id = this.editForm.parent_id || null;
        if (this.editForm.owner_agent_id !== (this.editGoal.owner_agent_id || '')) body.owner_agent_id = this.editForm.owner_agent_id || null;
        if (parseInt(this.editForm.progress, 10) !== this.editGoal.progress) body.progress = parseInt(this.editForm.progress, 10);
        await OpenFangAPI.put('/api/goals/' + this.editGoal.id, body);
        this.editGoal = null;
        OpenFangToast.success('Goal updated');
        await this.loadData();
      } catch(e) {
        OpenFangToast.error('Failed to update goal: ' + (e.message || e));
      }
      this.saving = false;
    },

    deleteGoal(goal) {
      var self = this;
      OpenFangToast.confirm('Delete Goal', 'Delete "' + goal.title + '"? Children will become root goals.', async function() {
        try {
          await OpenFangAPI.del('/api/goals/' + goal.id);
          OpenFangToast.success('Goal deleted');
          await self.loadData();
        } catch(e) {
          OpenFangToast.error('Failed to delete goal: ' + (e.message || e));
        }
      });
    },

    async setStatus(goal, status) {
      try {
        var progress = status === 'completed' ? 100 : goal.progress;
        await OpenFangAPI.put('/api/goals/' + goal.id, { status: status, progress: progress });
        await this.loadData();
      } catch(e) {
        OpenFangToast.error('Failed to update status: ' + (e.message || e));
      }
    },

    // ── Display helpers ──

    levelBadgeClass(level) {
      var map = { mission: 'badge-info', strategy: 'badge-created', objective: 'badge-warn', task: 'badge-dim' };
      return map[level] || 'badge-dim';
    },

    statusBadgeClass(status) {
      var map = { planned: 'badge-dim', active: 'badge-info', completed: 'badge-success', paused: 'badge-warn' };
      return map[status] || 'badge-dim';
    },

    levelIcon(level) {
      var map = { mission: '\u{1F3AF}', strategy: '\u{1F9ED}', objective: '\u{1F4CC}', task: '\u2705' };
      return map[level] || '\u{1F4CB}';
    },

    goalTitle(id) {
      if (!id) return '(none)';
      for (var i = 0; i < this.goals.length; i++) {
        if (this.goals[i].id === id) return this.goals[i].title;
      }
      return id.substring(0, 8) + '...';
    },

    get availableAgents() {
      return Alpine.store('app').agents || [];
    },

    agentName(agentId) {
      if (!agentId) return '(unassigned)';
      var agents = this.availableAgents;
      for (var i = 0; i < agents.length; i++) {
        if (agents[i].id === agentId) return agents[i].name;
      }
      return agentId.substring(0, 8) + '...';
    },

    relativeTime(ts) {
      if (!ts) return '';
      try {
        var diff = Date.now() - new Date(ts).getTime();
        if (isNaN(diff)) return '';
        if (diff < 60000) return 'just now';
        if (diff < 3600000) return Math.floor(diff / 60000) + 'm ago';
        if (diff < 86400000) return Math.floor(diff / 3600000) + 'h ago';
        return Math.floor(diff / 86400000) + 'd ago';
      } catch(e) { return ''; }
    },

    // Stats
    completedCount() { return this.goals.filter(function(g) { return g.status === 'completed'; }).length; },
    activeCount() { return this.goals.filter(function(g) { return g.status === 'active'; }).length; },
    avgProgress() {
      if (!this.goals.length) return 0;
      var sum = 0;
      for (var i = 0; i < this.goals.length; i++) sum += this.goals[i].progress;
      return Math.round(sum / this.goals.length);
    }
  };
}
