// OpenFang Users Page — User listing + per-user Memory management
'use strict';

function usersPage() {
  return {
    // -- State --
    users: [],
    defaultUserId: '',
    selectedUserId: '',
    tab: 'memory', // 'memory' | 'audit'
    loadingUsers: true,
    loadError: '',

    // Memory list (general topics)
    topics: [],
    memLoading: false,
    memError: '',

    // Topic viewer modal
    viewingTopic: null,
    viewingContent: '',
    viewingSummary: '',
    viewingUpdatedAt: '',
    viewingLoading: false,

    // Audit log
    auditEntries: [],
    auditLoading: false,
    auditError: '',

    // -- Init / Users list --
    async loadUsers() {
      this.loadingUsers = true;
      this.loadError = '';
      try {
        var data = await OpenFangAPI.get('/api/users');
        this.users = data.users || [];
        this.defaultUserId = data.default_user_id || '';
        if (!this.selectedUserId && this.defaultUserId) {
          this.selectedUserId = this.defaultUserId;
        }
        if (this.selectedUserId) await this.loadTopics();
      } catch (e) {
        this.users = [];
        this.loadError = (e && e.message) || 'Failed to load users.';
      }
      this.loadingUsers = false;
    },

    async loadData() { return this.loadUsers(); },

    selectUser(id) {
      this.selectedUserId = id;
      this.topics = [];
      this.auditEntries = [];
      if (this.tab === 'memory') this.loadTopics();
      else this.loadAudit();
    },

    get selectedUser() {
      var id = this.selectedUserId;
      return this.users.find(function (u) { return u.id === id; }) || null;
    },

    // -- Memory topics --
    async loadTopics() {
      if (!this.selectedUserId) { this.topics = []; return; }
      this.memLoading = true;
      this.memError = '';
      try {
        var data = await OpenFangAPI.get('/api/users/' + this.selectedUserId + '/memory');
        this.topics = data.topics || [];
      } catch (e) {
        this.topics = [];
        this.memError = (e && e.message) || 'Could not load memory topics.';
      }
      this.memLoading = false;
    },

    async viewTopic(topic) {
      if (!this.selectedUserId) return;
      this.viewingTopic = topic;
      this.viewingContent = '';
      this.viewingLoading = true;
      try {
        var data = await OpenFangAPI.get(
          '/api/users/' + this.selectedUserId + '/memory/' + encodeURIComponent(topic)
        );
        this.viewingContent = data.content || '';
        this.viewingSummary = data.summary || '';
        this.viewingUpdatedAt = data.updated_at || '';
      } catch (e) {
        this.viewingContent = '[error loading topic: ' + ((e && e.message) || 'unknown') + ']';
        this.viewingSummary = '';
        this.viewingUpdatedAt = '';
      }
      this.viewingLoading = false;
    },

    closeViewer() {
      this.viewingTopic = null;
      this.viewingContent = '';
      this.viewingSummary = '';
      this.viewingUpdatedAt = '';
    },

    deleteTopic(topic) {
      if (!this.selectedUserId) return;
      var self = this;
      OpenFangToast.confirm(
        'Delete Memory Topic',
        'Delete topic "' + topic + '"? This cannot be undone.',
        async function () {
          try {
            await OpenFangAPI.del(
              '/api/users/' + self.selectedUserId + '/memory/' + encodeURIComponent(topic)
            );
            OpenFangToast.success('Topic "' + topic + '" deleted');
            await self.loadTopics();
          } catch (e) {
            OpenFangToast.error('Failed to delete: ' + ((e && e.message) || 'unknown'));
          }
        }
      );
    },

    deleteAllMemory() {
      if (!this.selectedUserId) return;
      var self = this;
      var who = (this.selectedUser && this.selectedUser.name) || this.selectedUserId;
      OpenFangToast.confirm(
        'Wipe ALL Memory',
        'This will permanently delete every memory topic for "' + who +
        '" — both general and per-agent memory. This cannot be undone.',
        async function () {
          try {
            await OpenFangAPI.del('/api/users/' + self.selectedUserId + '/memory');
            OpenFangToast.success('All memory wiped for ' + who);
            await self.loadTopics();
          } catch (e) {
            OpenFangToast.error('Failed to wipe: ' + ((e && e.message) || 'unknown'));
          }
        }
      );
    },

    async exportMemory() {
      if (!this.selectedUserId) return;
      try {
        var data = await OpenFangAPI.get('/api/users/' + this.selectedUserId + '/memory/export');
        var json = JSON.stringify(data, null, 2);
        var blob = new Blob([json], { type: 'application/json' });
        var url = URL.createObjectURL(blob);
        var a = document.createElement('a');
        a.href = url;
        var date = new Date().toISOString().slice(0, 10).replace(/-/g, '');
        a.download = 'memory_' + this.selectedUserId + '_' + date + '.json';
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
      } catch (e) {
        OpenFangToast.error('Export failed: ' + ((e && e.message) || 'unknown'));
      }
    },

    // -- Audit log --
    async loadAudit() {
      if (!this.selectedUserId) { this.auditEntries = []; return; }
      this.auditLoading = true;
      this.auditError = '';
      try {
        var data = await OpenFangAPI.get(
          '/api/users/' + this.selectedUserId + '/memory/audit?limit=100'
        );
        this.auditEntries = data.entries || [];
      } catch (e) {
        this.auditEntries = [];
        this.auditError = (e && e.message) || 'Could not load audit.';
      }
      this.auditLoading = false;
    },

    switchTab(t) {
      this.tab = t;
      if (t === 'memory' && !this.topics.length && !this.memLoading) this.loadTopics();
      if (t === 'audit' && !this.auditEntries.length && !this.auditLoading) this.loadAudit();
    },

    formatAge(iso) {
      if (!iso) return '';
      try {
        var d = new Date(iso);
        var secs = Math.max(0, (Date.now() - d.getTime()) / 1000);
        if (secs < 60) return 'just now';
        if (secs < 3600) return Math.floor(secs / 60) + ' min ago';
        if (secs < 86400) return Math.floor(secs / 3600) + ' hr ago';
        if (secs < 86400 * 7) return Math.floor(secs / 86400) + ' days ago';
        if (secs < 86400 * 30) return Math.floor(secs / (86400 * 7)) + ' weeks ago';
        return Math.floor(secs / (86400 * 30)) + ' months ago';
      } catch (e) { return iso; }
    }
  };
}
