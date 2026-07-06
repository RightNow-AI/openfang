// OMTAE Brain Page — Obsidian vault browser (leads, skiptrace reports)
'use strict';

function brainPage() {
  return {
    status: {},
    currentPath: '',
    entries: [],
    recentLeads: [],
    selectedFile: null,
    fileContent: '',
    fileMeta: {},
    loading: true,
    fileLoading: false,
    loadError: '',
    pathStack: [],

    async loadBrain() {
      this.loading = true;
      this.loadError = '';
      try {
        var status = await OMTAEAPI.get('/api/brain/status');
        this.status = status || {};
        await this.loadDirectory('');
      } catch (e) {
        this.loadError = e.message || 'Could not load brain vault.';
      }
      this.loading = false;
    },

    async loadDirectory(path) {
      var q = path ? ('?path=' + encodeURIComponent(path)) : '';
      var data = await OMTAEAPI.get('/api/brain/list' + q);
      this.currentPath = data.path || path || '';
      this.entries = data.entries || [];
      this.recentLeads = data.recent_leads || [];
      if (data.vault_path) {
        this.status.vault_path = data.vault_path;
        this.status.vault_name = data.vault_name;
      }
      this.pathStack = this.currentPath ? this.currentPath.split('/') : [];
    },

    async openEntry(entry) {
      if (!entry) return;
      if (entry.kind === 'dir') {
        this.selectedFile = null;
        this.fileContent = '';
        await this.loadDirectory(entry.path);
        return;
      }
      await this.openFile(entry.path);
    },

    async openFile(path) {
      this.fileLoading = true;
      this.selectedFile = path;
      try {
        var data = await OMTAEAPI.get('/api/brain/file?path=' + encodeURIComponent(path));
        this.fileContent = data.content || '';
        this.fileMeta = data;
      } catch (e) {
        this.fileContent = '';
        if (typeof OMTAEToast !== 'undefined') OMTAEToast.error(e.message || 'Could not open file');
      }
      this.fileLoading = false;
    },

    async goUp() {
      if (!this.currentPath) return;
      var parts = this.currentPath.split('/');
      parts.pop();
      await this.loadDirectory(parts.join('/'));
    },

    async goRoot() {
      this.selectedFile = null;
      this.fileContent = '';
      await this.loadDirectory('');
    },

    async goToCrumb(index) {
      var parts = this.pathStack.slice(0, index + 1);
      await this.loadDirectory(parts.join('/'));
    },

    obsidianLink(path) {
      var vault = (this.status && this.status.vault_name) || 'omtae-brain';
      var rel = path || this.selectedFile || '';
      return 'obsidian://open?vault=' + encodeURIComponent(vault) + '&file=' + encodeURIComponent(rel);
    },

    renderPreview(text) {
      return typeof renderMarkdown === 'function' ? renderMarkdown(text) : escapeHtml(text);
    },

    formatModified(entry) {
      if (!entry || !entry.modified) return '';
      try {
        return new Date(entry.modified).toLocaleString();
      } catch (e) {
        return entry.modified;
      }
    }
  };
}
