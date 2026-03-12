function swePage() {
  return {
    loading: false,
    sweTasks: [],
    newTask: '',
    submitting: false,
    submitError: null,

    get activeTasks() {
      return this.sweTasks.filter(t => t.status === 'running' || t.status === 'pending');
    },

    get queuedTasks() {
      return this.sweTasks.filter(t => t.status === 'queued');
    },

    get completedTasks() {
      return this.sweTasks.filter(t => t.status === 'completed');
    },

    get failedTasks() {
      return this.sweTasks.filter(t => t.status === 'failed');
    },

    get historyTasks() {
      return this.sweTasks.filter(t => 
        t.status === 'completed' || t.status === 'failed' || t.status === 'cancelled'
      );
    },

    async fetchSWETasks() {
      this.loading = true;
      try {
        const resp = await fetch('/api/swe/tasks');
        if (resp.ok) {
          const data = await resp.json();
          this.sweTasks = data.tasks || data || [];
        } else {
          this.sweTasks = [];
        }
      } catch (e) {
        this.sweTasks = [];
      } finally {
        this.loading = false;
      }
    },

    async submitTask() {
      if (!this.newTask.trim() || this.submitting) return;
      this.submitting = true;
      this.submitError = null;
      try {
        const resp = await fetch('/api/swe/tasks', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ description: this.newTask.trim() }),
        });
        if (!resp.ok) {
          const err = await resp.json().catch(() => ({}));
          throw new Error(err.error || `HTTP ${resp.status}`);
        }
        this.newTask = '';
        window.showToast?.('Task created successfully', 'success');
        await this.fetchSWETasks();
      } catch (e) {
        this.submitError = e.message;
      } finally {
        this.submitting = false;
      }
    },

    async cancelTask(taskId) {
      try {
        const resp = await fetch(`/api/swe/tasks/${encodeURIComponent(taskId)}/cancel`, {
          method: 'POST',
        });
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        window.showToast?.('Task cancelled', 'success');
        await this.fetchSWETasks();
      } catch (e) {
        window.showToast?.(`Cancel failed: ${e.message}`, 'error');
      }
    },

    async retryTask(taskId) {
      try {
        const resp = await fetch(`/api/swe/tasks/${encodeURIComponent(taskId)}/retry`, {
          method: 'POST',
        });
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        window.showToast?.('Task retry initiated', 'success');
        await this.fetchSWETasks();
      } catch (e) {
        window.showToast?.(`Retry failed: ${e.message}`, 'error');
      }
    },

    viewTask(taskId) {
      window.location.hash = `#swe/${taskId}`;
    },

    async clearHistory() {
      this.sweTasks = this.sweTasks.filter(t => 
        t.status !== 'completed' && t.status !== 'failed' && t.status !== 'cancelled'
      );
      window.showToast?.('History cleared', 'info');
    },

    statusBadgeClass(status) {
      switch (status) {
        case 'running': return 'badge-info';
        case 'pending': return 'badge-dim';
        case 'queued': return 'badge-dim';
        case 'completed': return 'badge-success';
        case 'failed': return 'badge-error';
        case 'cancelled': return 'badge-dim';
        default: return 'badge-dim';
      }
    },

    formatTime(ts) {
      if (!ts) return '-';
      try {
        return new Date(ts).toLocaleString();
      } catch {
        return ts;
      }
    },
  };
}