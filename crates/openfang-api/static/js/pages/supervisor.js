// OpenFang Supervisor Page — MAESTRO orchestration dashboard
'use strict';
function supervisorPage() {
  return {
    tab: 'status',
    // -- Status state --
    status: null,
    loading: true,
    error: '',
    // -- History state --
    historyList: [],
    histLoading: false,
    // -- Learnings state --
    learningsList: [],
    learnLoading: false,
    // -- Config state --
    configData: null,
    cfgLoading: false,
    // -- Orchestration form --
    showOrchForm: false,
    orchTask: '',
    orchCaps: '',
    orchSubmitting: false,
    orchError: '',
    // -- Run detail modal --
    selectedRun: null,

    async loadStatus() {
      this.loading = true;
      this.error = '';
      try {
        const r = await api('/api/supervisor/status');
        if (r.ok) {
          this.status = await r.json();
        } else if (r.status === 503) {
          this.error = 'Supervisor not configured. Enable it in Settings → Analytics.';
        } else {
          this.error = 'Failed to load status: ' + r.status;
        }
      } catch (e) {
        this.error = 'Connection error: ' + e.message;
      }
      this.loading = false;
    },

    async loadHistory() {
      this.histLoading = true;
      try {
        const r = await api('/api/supervisor/history');
        if (r.ok) this.historyList = await r.json();
      } catch (e) { /* ignore */ }
      this.histLoading = false;
    },

    async loadLearnings() {
      this.learnLoading = true;
      try {
        const r = await api('/api/supervisor/learnings');
        if (r.ok) this.learningsList = await r.json();
      } catch (e) { /* ignore */ }
      this.learnLoading = false;
    },

    async loadConfig() {
      this.cfgLoading = true;
      try {
        const r = await api('/api/supervisor/config');
        if (r.ok) this.configData = await r.json();
      } catch (e) { /* ignore */ }
      this.cfgLoading = false;
    },

    async submitOrchestration() {
      if (!this.orchTask.trim()) return;
      this.orchSubmitting = true;
      this.orchError = '';
      try {
        const caps = this.orchCaps
          ? this.orchCaps.split(',').map(s => s.trim()).filter(Boolean)
          : [];
        const body = {
          task: this.orchTask.trim(),
          capabilities: caps.length ? caps : undefined,
        };
        const r = await api('/api/supervisor/orchestrate', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(body),
        });
        if (r.ok) {
          const result = await r.json();
          this.showOrchForm = false;
          this.orchTask = '';
          this.orchCaps = '';
          // Refresh status to show the new run
          await this.loadStatus();
          // Show the result
          this.selectedRun = result;
        } else {
          const text = await r.text();
          this.orchError = 'Failed: ' + (text || r.status);
        }
      } catch (e) {
        this.orchError = 'Error: ' + e.message;
      }
      this.orchSubmitting = false;
    },

    async viewRun(runId) {
      try {
        const r = await api('/api/supervisor/runs/' + runId);
        if (r.ok) {
          this.selectedRun = await r.json();
        }
      } catch (e) { /* ignore */ }
    },
  };
}
