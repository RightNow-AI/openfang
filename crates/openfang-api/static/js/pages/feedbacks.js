// OpenFang Feedbacks Page - read-only feedback task visibility
'use strict';

function feedbacksPage() {
  return {
    tasks: [],
    selectedId: '',
    loading: true,
    loadError: '',
    statusFilter: '',

    async loadData() {
      this.loading = true;
      this.loadError = '';
      try {
        var data = await OpenFangAPI.get('/api/feedbacks');
        this.tasks = Array.isArray(data) ? data : (data.tasks || []);
        if (!this.selectedId && this.tasks.length > 0) {
          this.selectedId = this.tasks[0].id;
        }
        if (this.selectedId && !this.selectedTask()) {
          this.selectedId = this.tasks.length > 0 ? this.tasks[0].id : '';
        }
      } catch(e) {
        this.loadError = e.message || 'Could not load feedback tasks.';
      }
      this.loading = false;
    },

    filteredTasks() {
      var self = this;
      if (!this.statusFilter) return this.tasks;
      return this.tasks.filter(function(task) { return task.status === self.statusFilter; });
    },

    selectedTask() {
      for (var i = 0; i < this.tasks.length; i++) {
        if (this.tasks[i].id === this.selectedId) return this.tasks[i];
      }
      return null;
    },

    selectTask(task) {
      this.selectedId = task && task.id ? task.id : '';
    },

    payloadOf(task) {
      if (!task || !task.payload || typeof task.payload !== 'object') return {};
      return task.payload;
    },

    field(payload, name, fallback) {
      if (!payload || payload[name] === undefined || payload[name] === null || payload[name] === '') {
        return fallback || '-';
      }
      return payload[name];
    },

    countByStatus(status) {
      return this.tasks.filter(function(task) { return task.status === status; }).length;
    },

    statusBadgeClass(status) {
      switch(status) {
        case 'completed': return 'badge badge-success';
        case 'in_progress': return 'badge badge-info';
        case 'failed': return 'badge badge-error';
        case 'pending': return 'badge badge-warn';
        default: return 'badge badge-dim';
      }
    },

    signalBadgeClass(signal) {
      switch(signal) {
        case 'positive': return 'badge badge-success';
        case 'negative': return 'badge badge-error';
        case 'mixed': return 'badge badge-warn';
        case 'unclear': return 'badge badge-dim';
        default: return 'badge badge-info';
      }
    },

    displayStatus(status) {
      if (status === 'in_progress') return 'In Progress';
      return status ? status.charAt(0).toUpperCase() + status.slice(1) : 'Unknown';
    },

    formatDate(dateStr) {
      if (!dateStr) return '-';
      var d = new Date(dateStr);
      if (isNaN(d.getTime())) return dateStr;
      return d.toLocaleString();
    },

    shortId(id) {
      if (!id) return '-';
      return id.length > 12 ? id.slice(0, 8) + '...' + id.slice(-4) : id;
    },

    resultHtml(task) {
      if (!task || !task.result) return '';
      return renderMarkdown(task.result);
    },

    hasResult(task) {
      return !!(task && task.result && String(task.result).trim());
    }
  };
}
