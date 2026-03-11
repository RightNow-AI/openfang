// Operis Scheduler Page — Cron job management + event triggers unified view
'use strict';

function schedulerPage() {
  return {
    tab: 'jobs',

    // -- Scheduled Jobs state --
    jobs: [],
    loading: true,
    loadError: '',

    // -- Event Triggers state --
    triggers: [],
    trigLoading: false,
    trigLoadError: '',

    // -- Run History state --
    history: [],
    historyLoading: false,

    // -- Create Job form --
    showCreateForm: false,
    newJob: {
      name: '',
      cron: '',
      agent_id: '',
      message: '',
      enabled: true
    },
    creating: false,

    // -- Run Now state --
    runningJobId: '',

    // Cron presets
    cronPresets: [
      { label: 'Every minute', cron: '* * * * *' },
      { label: 'Every 5 minutes', cron: '*/5 * * * *' },
      { label: 'Every 15 minutes', cron: '*/15 * * * *' },
      { label: 'Every 30 minutes', cron: '*/30 * * * *' },
      { label: 'Every hour', cron: '0 * * * *' },
      { label: 'Every 6 hours', cron: '0 */6 * * *' },
      { label: 'Daily at midnight', cron: '0 0 * * *' },
      { label: 'Daily at 9am', cron: '0 9 * * *' },
      { label: 'Weekdays at 9am', cron: '0 9 * * 1-5' },
      { label: 'Every Monday 9am', cron: '0 9 * * 1' },
      { label: 'First of month', cron: '0 0 1 * *' }
    ],

    // ── Lifecycle ──

    async loadData() {
      this.loading = true;
      this.loadError = '';
      try {
        await this.loadJobs();
      } catch(e) {
        this.loadError = e.message || 'Could not load scheduler data.';
      }
      this.loading = false;
    },

    async loadJobs() {
      var data = await OperisAPI.get('/api/schedules');
      this.jobs = data.schedules || [];
    },

    async loadTriggers() {
      this.trigLoading = true;
      this.trigLoadError = '';
      try {
        var data = await OperisAPI.get('/api/triggers');
        this.triggers = Array.isArray(data) ? data : [];
      } catch(e) {
        this.triggers = [];
        this.trigLoadError = e.message || 'Could not load triggers.';
      }
      this.trigLoading = false;
    },

    async loadHistory() {
      this.historyLoading = true;
      try {
        // Build history from jobs with run data + recent audit entries
        var historyItems = [];

        // Add job run info from schedule data
        var jobs = this.jobs || [];
        for (var i = 0; i < jobs.length; i++) {
          var job = jobs[i];
          if (job.last_run) {
            historyItems.push({
              timestamp: job.last_run,
              name: job.name || job.description || '(unnamed)',
              type: 'schedule',
              status: 'completed',
              run_count: job.run_count || 0
            });
          }
        }

        // Also load trigger fire counts
        var triggers = this.triggers || [];
        for (var j = 0; j < triggers.length; j++) {
          var t = triggers[j];
          if (t.fire_count > 0) {
            historyItems.push({
              timestamp: t.created_at,
              name: 'Trigger: ' + this.triggerType(t.pattern),
              type: 'trigger',
              status: 'fired',
              run_count: t.fire_count
            });
          }
        }

        // Sort by timestamp descending
        historyItems.sort(function(a, b) {
          return new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime();
        });

        this.history = historyItems;
      } catch(e) {
        this.history = [];
      }
      this.historyLoading = false;
    },

    // ── Job CRUD ──

    async createJob() {
      if (!this.newJob.name.trim()) {
        OperisToast.warn('Please enter a job name');
        return;
      }
      if (!this.newJob.cron.trim()) {
        OperisToast.warn('Please enter a cron expression');
        return;
      }
      this.creating = true;
      try {
        var jobName = this.newJob.name;
        await OperisAPI.post('/api/schedules', {
          name: this.newJob.name,
          cron: this.newJob.cron,
          agent_id: this.newJob.agent_id,
          message: this.newJob.message,
          enabled: this.newJob.enabled
        });
        this.showCreateForm = false;
        this.newJob = { name: '', cron: '', agent_id: '', message: '', enabled: true };
        OperisToast.success('Schedule "' + jobName + '" created');
        await this.loadJobs();
      } catch(e) {
        OperisToast.error('Failed to create schedule: ' + (e.message || e));
      }
      this.creating = false;
    },

    async toggleJob(job) {
      try {
        var newState = !job.enabled;
        await OperisAPI.put('/api/schedules/' + job.id, { enabled: newState });
        job.enabled = newState;
        OperisToast.success('Schedule ' + (newState ? 'enabled' : 'paused'));
      } catch(e) {
        OperisToast.error('Failed to toggle schedule: ' + (e.message || e));
      }
    },

    deleteJob(job) {
      var self = this;
      var jobName = job.name || job.id;
      OperisToast.confirm('Delete Schedule', 'Delete "' + jobName + '"? This cannot be undone.', async function() {
        try {
          await OperisAPI.del('/api/schedules/' + job.id);
          self.jobs = self.jobs.filter(function(j) { return j.id !== job.id; });
          OperisToast.success('Schedule "' + jobName + '" deleted');
        } catch(e) {
          OperisToast.error('Failed to delete schedule: ' + (e.message || e));
        }
      });
    },

    async runNow(job) {
      this.runningJobId = job.id;
      try {
        var result = await OperisAPI.post('/api/schedules/' + job.id + '/run', {});
        if (result.status === 'completed') {
          OperisToast.success('Schedule "' + (job.name || 'job') + '" executed successfully');
          // Update the job's last_run locally
          job.last_run = new Date().toISOString();
          job.run_count = (job.run_count || 0) + 1;
        } else {
          OperisToast.error('Schedule run failed: ' + (result.error || 'Unknown error'));
        }
      } catch(e) {
        OperisToast.error('Failed to run schedule: ' + (e.message || e));
      }
      this.runningJobId = '';
    },

    // ── Trigger helpers (reused from workflows page) ──

    triggerType(pattern) {
      if (!pattern) return 'unknown';
      if (typeof pattern === 'string') return pattern;
      var keys = Object.keys(pattern);
      if (keys.length === 0) return 'unknown';
      var key = keys[0];
      var names = {
        lifecycle: 'Lifecycle',
        agent_spawned: 'Agent Spawned',
        agent_terminated: 'Agent Terminated',
        system: 'System',
        system_keyword: 'System Keyword',
        memory_update: 'Memory Update',
        memory_key_pattern: 'Memory Key',
        all: 'All Events',
        content_match: 'Content Match'
      };
      return names[key] || key.replace(/_/g, ' ');
    },

    async toggleTrigger(trigger) {
      try {
        var newState = !trigger.enabled;
        await OperisAPI.put('/api/triggers/' + trigger.id, { enabled: newState });
        trigger.enabled = newState;
        OperisToast.success('Trigger ' + (newState ? 'enabled' : 'disabled'));
      } catch(e) {
        OperisToast.error('Failed to toggle trigger: ' + (e.message || e));
      }
    },

    deleteTrigger(trigger) {
      var self = this;
      OperisToast.confirm('Delete Trigger', 'Delete this trigger? This cannot be undone.', async function() {
        try {
          await OperisAPI.del('/api/triggers/' + trigger.id);
          self.triggers = self.triggers.filter(function(t) { return t.id !== trigger.id; });
          OperisToast.success('Trigger deleted');
        } catch(e) {
          OperisToast.error('Failed to delete trigger: ' + (e.message || e));
        }
      });
    },

    // ── Utility ──

    get availableAgents() {
      return Alpine.store('app').agents || [];
    },

    agentName(agentId) {
      if (!agentId) return '(any)';
      var agents = this.availableAgents;
      for (var i = 0; i < agents.length; i++) {
        if (agents[i].id === agentId) return agents[i].name;
      }
      // Truncate UUID
      if (agentId.length > 12) return agentId.substring(0, 8) + '...';
      return agentId;
    },

    describeCron(expr) {
      if (!expr) return '';
      var map = {
        '* * * * *': 'Every minute',
        '*/2 * * * *': 'Every 2 minutes',
        '*/5 * * * *': 'Every 5 minutes',
        '*/10 * * * *': 'Every 10 minutes',
        '*/15 * * * *': 'Every 15 minutes',
        '*/30 * * * *': 'Every 30 minutes',
        '0 * * * *': 'Every hour',
        '0 */2 * * *': 'Every 2 hours',
        '0 */4 * * *': 'Every 4 hours',
        '0 */6 * * *': 'Every 6 hours',
        '0 */12 * * *': 'Every 12 hours',
        '0 0 * * *': 'Daily at midnight',
        '0 6 * * *': 'Daily at 6:00 AM',
        '0 9 * * *': 'Daily at 9:00 AM',
        '0 12 * * *': 'Daily at noon',
        '0 18 * * *': 'Daily at 6:00 PM',
        '0 9 * * 1-5': 'Weekdays at 9:00 AM',
        '0 9 * * 1': 'Mondays at 9:00 AM',
        '0 0 * * 0': 'Sundays at midnight',
        '0 0 1 * *': '1st of every month',
        '0 0 * * 1': 'Mondays at midnight'
      };
      if (map[expr]) return map[expr];

      // Try to parse common patterns
      var parts = expr.split(' ');
      if (parts.length !== 5) return expr;

      var min = parts[0];
      var hour = parts[1];
      var dom = parts[2];
      var mon = parts[3];
      var dow = parts[4];

      // "*/N * * * *" patterns
      if (min.indexOf('*/') === 0 && hour === '*' && dom === '*' && mon === '*' && dow === '*') {
        return 'Every ' + min.substring(2) + ' minutes';
      }
      // "0 */N * * *" patterns
      if (min === '0' && hour.indexOf('*/') === 0 && dom === '*' && mon === '*' && dow === '*') {
        return 'Every ' + hour.substring(2) + ' hours';
      }
      // "M H * * *" — daily at specific time
      if (dom === '*' && mon === '*' && dow === '*' && min.match(/^\d+$/) && hour.match(/^\d+$/)) {
        var h = parseInt(hour, 10);
        var m = parseInt(min, 10);
        var ampm = h >= 12 ? 'PM' : 'AM';
        var h12 = h === 0 ? 12 : (h > 12 ? h - 12 : h);
        var mStr = m < 10 ? '0' + m : '' + m;
        return 'Daily at ' + h12 + ':' + mStr + ' ' + ampm;
      }

      return expr;
    },

    applyCronPreset(preset) {
      this.newJob.cron = preset.cron;
    },

    formatTime(ts) {
      if (!ts) return '-';
      try {
        var d = new Date(ts);
        if (isNaN(d.getTime())) return '-';
        return d.toLocaleString();
      } catch(e) { return '-'; }
    },

    relativeTime(ts) {
      if (!ts) return 'never';
      try {
        var diff = Date.now() - new Date(ts).getTime();
        if (isNaN(diff)) return 'never';
        if (diff < 0) return 'just now';
        if (diff < 60000) return 'just now';
        if (diff < 3600000) return Math.floor(diff / 60000) + 'm ago';
        if (diff < 86400000) return Math.floor(diff / 3600000) + 'h ago';
        return Math.floor(diff / 86400000) + 'd ago';
      } catch(e) { return 'never'; }
    },

    jobCount() {
      var enabled = 0;
      for (var i = 0; i < this.jobs.length; i++) {
        if (this.jobs[i].enabled) enabled++;
      }
      return enabled;
    },

    triggerCount() {
      var enabled = 0;
      for (var i = 0; i < this.triggers.length; i++) {
        if (this.triggers[i].enabled) enabled++;
      }
      return enabled;
    }
  };
}
