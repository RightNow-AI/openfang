// OpenFang Extensions Page — browse, install, manage MCP integrations
'use strict';

function extensionsPage() {
  return {
    tab: 'available',
    available: [],
    installed: [],
    health: [],
    loading: true,
    loadError: '',
    installingId: null,
    removingId: null,
    reconnectingId: null,
    searchQuery: '',
    selectedCategory: '',

    async loadData() {
      this.loading = true;
      this.loadError = '';
      try {
        await Promise.all([this.loadAvailable(), this.loadInstalled()]);
      } catch (e) {
        this.loadError = e.message || 'Could not load extensions.';
      }
      this.loading = false;
    },

    async loadAvailable() {
      var data = await OpenFangAPI.get('/api/integrations/available');
      this.available = data.integrations || [];
    },

    async loadInstalled() {
      var data = await OpenFangAPI.get('/api/integrations');
      this.installed = data.installed || [];
    },

    async loadHealth() {
      try {
        var data = await OpenFangAPI.get('/api/integrations/health');
        this.health = data.health || [];
      } catch (e) {
        this.health = [];
      }
    },

    get categories() {
      var cats = {};
      for (var i = 0; i < this.available.length; i++) {
        var c = this.available[i].category || 'Other';
        cats[c] = (cats[c] || 0) + 1;
      }
      return cats;
    },

    get filteredAvailable() {
      var self = this;
      return this.available.filter(function (ext) {
        if (self.selectedCategory && ext.category !== self.selectedCategory) return false;
        if (!self.searchQuery) return true;
        var q = self.searchQuery.toLowerCase();
        return (
          ext.name.toLowerCase().indexOf(q) >= 0 ||
          ext.id.toLowerCase().indexOf(q) >= 0 ||
          ext.description.toLowerCase().indexOf(q) >= 0 ||
          (ext.tags || []).some(function (t) {
            return t.toLowerCase().indexOf(q) >= 0;
          })
        );
      });
    },

    isInstalled: function (id) {
      return this.installed.some(function (i) {
        return i.id === id;
      });
    },

    getHealth: function (id) {
      for (var i = 0; i < this.health.length; i++) {
        if (this.health[i].id === id) return this.health[i];
      }
      return null;
    },

    healthBadge: function (h) {
      if (!h) return { cls: 'badge-dim', text: 'Unknown' };
      var s = (h.status || '').toLowerCase();
      if (s === 'ready' || s.indexOf('ready') >= 0) return { cls: 'badge-success', text: 'Connected' };
      if (s.indexOf('error') >= 0) return { cls: 'badge-error', text: 'Error' };
      if (h.reconnecting) return { cls: 'badge-warn', text: 'Reconnecting' };
      return { cls: 'badge-dim', text: h.status || 'Unknown' };
    },

    async install(id) {
      this.installingId = id;
      try {
        await OpenFangAPI.post('/api/integrations/add', { id: id });
        OpenFangToast.success('Installed ' + id);
        await Promise.all([this.loadAvailable(), this.loadInstalled(), this.loadHealth()]);
      } catch (e) {
        OpenFangToast.error(e.message || 'Install failed');
      }
      this.installingId = null;
    },

    async remove(id) {
      var self = this;
      OpenFangToast.confirm('Remove Integration', 'Uninstall ' + id + '?', async function () {
        self.removingId = id;
        try {
          await OpenFangAPI.del('/api/integrations/' + encodeURIComponent(id));
          OpenFangToast.success('Removed ' + id);
          await Promise.all([self.loadAvailable(), self.loadInstalled(), self.loadHealth()]);
        } catch (e) {
          OpenFangToast.error(e.message || 'Remove failed');
        }
        self.removingId = null;
      });
    },

    async reconnect(id) {
      this.reconnectingId = id;
      try {
        var result = await OpenFangAPI.post('/api/integrations/' + encodeURIComponent(id) + '/reconnect', {});
        OpenFangToast.success(id + ' reconnected (' + (result.tool_count || 0) + ' tools)');
        await this.loadHealth();
      } catch (e) {
        OpenFangToast.error(e.message || 'Reconnect failed');
      }
      this.reconnectingId = null;
    },

    async reload() {
      try {
        var result = await OpenFangAPI.post('/api/integrations/reload', {});
        OpenFangToast.success('Reloaded (' + (result.new_connections || 0) + ' connections)');
        await Promise.all([this.loadInstalled(), this.loadHealth()]);
      } catch (e) {
        OpenFangToast.error(e.message || 'Reload failed');
      }
    },

    formatDate: function (iso) {
      if (!iso) return '';
      try {
        return new Date(iso).toLocaleString();
      } catch (e) {
        return iso;
      }
    }
  };
}
