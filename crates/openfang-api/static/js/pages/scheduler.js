// OpenFang Scheduler Page — Cron job management + event triggers unified view
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
      enabled: true,
      delivery_targets: []
    },
    creating: false,

    // -- Run Now state --
    runningJobId: '',

    // -- Delivery targets picker (create modal) --
    showTargetPicker: false,
    pickerType: 'channel',
    draftTarget: null,

    // -- Expanded job / delivery log state --
    expandedJobId: '',
    deliveryLog: { targets: [], entries: [] },
    deliveryLogLoading: false,
    deliveryLogError: '',

    // -- Edit targets state (per-existing-job) --
    editingTargetsJobId: '',
    editingTargets: [],
    savingTargets: false,

    // -- Available channel types (populated from /api/channels) --
    channelTypes: [],

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
        // Channels are optional — failure is non-fatal for the scheduler page.
        this.loadChannelTypes();
      } catch(e) {
        this.loadError = e.message || 'Could not load scheduler data.';
      }
      this.loading = false;
    },

    async loadChannelTypes() {
      try {
        var data = await OpenFangAPI.get('/api/channels');
        // /api/channels returns an array of channel descriptors; pull names.
        var list = Array.isArray(data) ? data : (data && data.channels) || [];
        var names = [];
        for (var i = 0; i < list.length; i++) {
          var ch = list[i];
          var name = ch && (ch.name || ch.display_name || ch.channel_type);
          if (name && names.indexOf(name) === -1) names.push(name);
        }
        this.channelTypes = names;
      } catch(e) {
        // Fall through silently — the form uses a plain input as fallback.
        this.channelTypes = [];
      }
    },

    async loadJobs() {
      var data = await OpenFangAPI.get('/api/cron/jobs');
      var raw = data.jobs || [];
      // Normalize cron API response to flat fields the UI expects
      this.jobs = raw.map(function(j) {
        var cron = '';
        if (j.schedule) {
          if (j.schedule.kind === 'cron') cron = j.schedule.expr || '';
          else if (j.schedule.kind === 'every') cron = 'every ' + j.schedule.every_secs + 's';
          else if (j.schedule.kind === 'at') cron = 'at ' + (j.schedule.at || '');
        }
        return {
          id: j.id,
          name: j.name,
          cron: cron,
          agent_id: j.agent_id,
          message: j.action ? j.action.message || '' : '',
          enabled: j.enabled,
          last_run: j.last_run,
          next_run: j.next_run,
          delivery: j.delivery ? j.delivery.kind || '' : '',
          delivery_targets: Array.isArray(j.delivery_targets) ? j.delivery_targets : [],
          created_at: j.created_at
        };
      });
    },

    async loadTriggers() {
      this.trigLoading = true;
      this.trigLoadError = '';
      try {
        var data = await OpenFangAPI.get('/api/triggers');
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
        var historyItems = [];
        var jobs = this.jobs || [];
        for (var i = 0; i < jobs.length; i++) {
          var job = jobs[i];
          if (job.last_run) {
            historyItems.push({
              timestamp: job.last_run,
              name: job.name || '(unnamed)',
              type: 'schedule',
              status: 'completed',
              run_count: 0
            });
          }
        }
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
        OpenFangToast.warn('Please enter a job name');
        return;
      }
      if (!this.newJob.cron.trim()) {
        OpenFangToast.warn('Please enter a cron expression');
        return;
      }
      this.creating = true;
      try {
        var jobName = this.newJob.name;
        var body = {
          agent_id: this.newJob.agent_id,
          name: this.newJob.name,
          schedule: { kind: 'cron', expr: this.newJob.cron },
          action: { kind: 'agent_turn', message: this.newJob.message || 'Scheduled task: ' + this.newJob.name },
          delivery: { kind: 'last_channel' },
          enabled: this.newJob.enabled
        };
        if (this.newJob.delivery_targets && this.newJob.delivery_targets.length) {
          body.delivery_targets = this.newJob.delivery_targets.map(this.sanitizeTarget);
        }
        await OpenFangAPI.post('/api/cron/jobs', body);
        this.showCreateForm = false;
        this.newJob = { name: '', cron: '', agent_id: '', message: '', enabled: true, delivery_targets: [] };
        OpenFangToast.success('Schedule "' + jobName + '" created');
        await this.loadJobs();
      } catch(e) {
        OpenFangToast.error('Failed to create schedule: ' + (e.message || e));
      }
      this.creating = false;
    },

    async toggleJob(job) {
      try {
        var newState = !job.enabled;
        await OpenFangAPI.put('/api/cron/jobs/' + job.id + '/enable', { enabled: newState });
        job.enabled = newState;
        OpenFangToast.success('Schedule ' + (newState ? 'enabled' : 'paused'));
      } catch(e) {
        OpenFangToast.error('Failed to toggle schedule: ' + (e.message || e));
      }
    },

    deleteJob(job) {
      var self = this;
      var jobName = job.name || job.id;
      OpenFangToast.confirm('Delete Schedule', 'Delete "' + jobName + '"? This cannot be undone.', async function() {
        try {
          await OpenFangAPI.del('/api/cron/jobs/' + job.id);
          self.jobs = self.jobs.filter(function(j) { return j.id !== job.id; });
          OpenFangToast.success('Schedule "' + jobName + '" deleted');
        } catch(e) {
          OpenFangToast.error('Failed to delete schedule: ' + (e.message || e));
        }
      });
    },

    async runNow(job) {
      this.runningJobId = job.id;
      try {
        var result = await OpenFangAPI.post('/api/cron/jobs/' + job.id + '/run', {});
        if (result.status === 'triggered' || result.status === 'completed') {
          OpenFangToast.success('Job "' + (job.name || 'job') + '" triggered');
          // Don't update job.last_run here — the job runs asynchronously in the
          // background. The real last_run is set by the server on completion and
          // will appear on the next data refresh.
        } else {
          OpenFangToast.error('Run failed: ' + (result.error || 'Unknown error'));
        }
      } catch(e) {
        OpenFangToast.error('Run failed: ' + (e.message || e));
      }
      this.runningJobId = '';
    },

    // ── Delivery target editing (create modal) ──

    openTargetPicker() {
      this.pickerType = 'channel';
      this.draftTarget = this.blankTarget('channel');
      this.showTargetPicker = true;
    },

    cancelTargetPicker() {
      this.showTargetPicker = false;
      this.draftTarget = null;
    },

    onPickerTypeChange() {
      this.draftTarget = this.blankTarget(this.pickerType);
    },

    blankTarget(type) {
      if (type === 'channel') {
        return { type: 'channel', channel_type: '', recipient: '' };
      }
      if (type === 'webhook') {
        return { type: 'webhook', url: '', auth_header: '' };
      }
      if (type === 'local_file') {
        return { type: 'local_file', path: '', append: false };
      }
      if (type === 'email') {
        return { type: 'email', to: '', subject_template: '' };
      }
      return null;
    },

    addDraftTarget() {
      var err = this.validateTarget(this.draftTarget);
      if (err) {
        OpenFangToast.warn(err);
        return;
      }
      if (!Array.isArray(this.newJob.delivery_targets)) this.newJob.delivery_targets = [];
      this.newJob.delivery_targets.push(this.sanitizeTarget(this.draftTarget));
      this.showTargetPicker = false;
      this.draftTarget = null;
    },

    removeTarget(idx) {
      if (!Array.isArray(this.newJob.delivery_targets)) return;
      this.newJob.delivery_targets.splice(idx, 1);
    },

    validateTarget(t) {
      if (!t || !t.type) return 'Pick a target type';
      if (t.type === 'channel') {
        if (!t.channel_type || !t.channel_type.trim()) return 'Channel type is required';
        if (!t.recipient || !t.recipient.trim()) return 'Recipient is required';
      } else if (t.type === 'webhook') {
        if (!t.url || !t.url.trim()) return 'Webhook URL is required';
        if (t.url.indexOf('http://') !== 0 && t.url.indexOf('https://') !== 0) {
          return 'Webhook URL must start with http:// or https://';
        }
      } else if (t.type === 'local_file') {
        if (!t.path || !t.path.trim()) return 'File path is required';
      } else if (t.type === 'email') {
        if (!t.to || !t.to.trim()) return 'Recipient email is required';
      }
      return null;
    },

    // Strip empty-string optional fields so serde accepts the payload cleanly.
    sanitizeTarget(t) {
      if (!t) return null;
      var out = { type: t.type };
      if (t.type === 'channel') {
        out.channel_type = (t.channel_type || '').trim();
        out.recipient = (t.recipient || '').trim();
      } else if (t.type === 'webhook') {
        out.url = (t.url || '').trim();
        if (t.auth_header && t.auth_header.trim()) out.auth_header = t.auth_header.trim();
      } else if (t.type === 'local_file') {
        out.path = (t.path || '').trim();
        out.append = !!t.append;
      } else if (t.type === 'email') {
        out.to = (t.to || '').trim();
        if (t.subject_template && t.subject_template.trim()) {
          out.subject_template = t.subject_template.trim();
        }
      }
      return out;
    },

    // ── Chip rendering helpers ──

    targetChipLabel(t) {
      if (!t || !t.type) return '?';
      if (t.type === 'channel') return 'CHANNEL: ' + (t.channel_type || '?');
      if (t.type === 'webhook') return 'WEBHOOK';
      if (t.type === 'local_file') return 'FILE: ' + this.truncate(t.path || '', 28);
      if (t.type === 'email') return 'EMAIL: ' + this.truncate(t.to || '', 24);
      return t.type.toUpperCase();
    },

    targetChipClass(t) {
      if (!t || !t.type) return 'badge-dim';
      if (t.type === 'channel') return 'badge-info';
      if (t.type === 'webhook') return 'badge-created';
      if (t.type === 'local_file') return 'badge-muted';
      if (t.type === 'email') return 'badge-warn';
      return 'badge-dim';
    },

    targetSummary(t) {
      if (!t) return '';
      if (t.type === 'channel') return (t.channel_type || '?') + ' -> ' + (t.recipient || '?');
      if (t.type === 'webhook') return t.url || '(no url)';
      if (t.type === 'local_file') return (t.append ? 'append ' : 'overwrite ') + (t.path || '');
      if (t.type === 'email') {
        var base = t.to || '';
        if (t.subject_template) base += ' · subject: ' + t.subject_template;
        return base;
      }
      return JSON.stringify(t);
    },

    // ── Expand row / delivery log ──

    async toggleExpand(job) {
      if (this.expandedJobId === job.id) {
        this.expandedJobId = '';
        return;
      }
      this.expandedJobId = job.id;
      this.deliveryLog = { targets: [], entries: [] };
      this.deliveryLogError = '';
      this.deliveryLogLoading = true;
      try {
        var data = await OpenFangAPI.get('/api/schedules/' + job.id + '/delivery-log');
        this.deliveryLog = {
          targets: Array.isArray(data.targets) ? data.targets : [],
          entries: Array.isArray(data.entries) ? data.entries : []
        };
      } catch(e) {
        this.deliveryLogError = e.message || 'Could not load delivery log.';
      }
      this.deliveryLogLoading = false;
    },

    // ── Edit targets on existing job ──

    startEditTargets(job) {
      this.editingTargetsJobId = job.id;
      // Clone so cancel doesn't mutate the loaded list.
      this.editingTargets = (job.delivery_targets || []).map(function(t) {
        return JSON.parse(JSON.stringify(t));
      });
      this.pickerType = 'channel';
      this.draftTarget = null;
      this.showTargetPicker = false;
    },

    cancelEditTargets() {
      this.editingTargetsJobId = '';
      this.editingTargets = [];
      this.draftTarget = null;
      this.showTargetPicker = false;
    },

    addEditTarget() {
      this.pickerType = 'channel';
      this.draftTarget = this.blankTarget('channel');
      this.showTargetPicker = true;
    },

    addDraftTargetToEdit() {
      var err = this.validateTarget(this.draftTarget);
      if (err) {
        OpenFangToast.warn(err);
        return;
      }
      this.editingTargets.push(this.sanitizeTarget(this.draftTarget));
      this.showTargetPicker = false;
      this.draftTarget = null;
    },

    removeEditTarget(idx) {
      this.editingTargets.splice(idx, 1);
    },

    async saveEditTargets() {
      if (!this.editingTargetsJobId) return;
      this.savingTargets = true;
      try {
        var clean = this.editingTargets.map(this.sanitizeTarget);
        await OpenFangAPI.put('/api/schedules/' + this.editingTargetsJobId, {
          delivery_targets: clean
        });
        OpenFangToast.success('Delivery targets updated');
        this.cancelEditTargets();
        await this.loadJobs();
      } catch(e) {
        OpenFangToast.error('Failed to update targets: ' + (e.message || e));
      }
      this.savingTargets = false;
    },

    // ── Trigger helpers ──

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
        await OpenFangAPI.put('/api/triggers/' + trigger.id, { enabled: newState });
        trigger.enabled = newState;
        OpenFangToast.success('Trigger ' + (newState ? 'enabled' : 'disabled'));
      } catch(e) {
        OpenFangToast.error('Failed to toggle trigger: ' + (e.message || e));
      }
    },

    deleteTrigger(trigger) {
      var self = this;
      OpenFangToast.confirm('Delete Trigger', 'Delete this trigger? This cannot be undone.', async function() {
        try {
          await OpenFangAPI.del('/api/triggers/' + trigger.id);
          self.triggers = self.triggers.filter(function(t) { return t.id !== trigger.id; });
          OpenFangToast.success('Trigger deleted');
        } catch(e) {
          OpenFangToast.error('Failed to delete trigger: ' + (e.message || e));
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
      if (agentId.length > 12) return agentId.substring(0, 8) + '...';
      return agentId;
    },

    describeCron(expr) {
      if (!expr) return '';
      // Handle non-cron schedule descriptions
      if (expr.indexOf('every ') === 0) return expr;
      if (expr.indexOf('at ') === 0) return 'One-time: ' + expr.substring(3);

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

      var parts = expr.split(' ');
      if (parts.length !== 5) return expr;

      var min = parts[0];
      var hour = parts[1];
      var dom = parts[2];
      var mon = parts[3];
      var dow = parts[4];

      if (min.indexOf('*/') === 0 && hour === '*' && dom === '*' && mon === '*' && dow === '*') {
        return 'Every ' + min.substring(2) + ' minutes';
      }
      if (min === '0' && hour.indexOf('*/') === 0 && dom === '*' && mon === '*' && dow === '*') {
        return 'Every ' + hour.substring(2) + ' hours';
      }

      var dowNames = { '0': 'Sun', '1': 'Mon', '2': 'Tue', '3': 'Wed', '4': 'Thu', '5': 'Fri', '6': 'Sat', '7': 'Sun',
                       '1-5': 'Weekdays', '0,6': 'Weekends', '6,0': 'Weekends' };

      if (dom === '*' && mon === '*' && min.match(/^\d+$/) && hour.match(/^\d+$/)) {
        var h = parseInt(hour, 10);
        var m = parseInt(min, 10);
        var ampm = h >= 12 ? 'PM' : 'AM';
        var h12 = h === 0 ? 12 : (h > 12 ? h - 12 : h);
        var mStr = m < 10 ? '0' + m : '' + m;
        var timeStr = h12 + ':' + mStr + ' ' + ampm;
        if (dow === '*') return 'Daily at ' + timeStr;
        var dowLabel = dowNames[dow] || ('DoW ' + dow);
        return dowLabel + ' at ' + timeStr;
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
        if (diff < 0) {
          // Future time
          var absDiff = Math.abs(diff);
          if (absDiff < 60000) return 'in <1m';
          if (absDiff < 3600000) return 'in ' + Math.floor(absDiff / 60000) + 'm';
          if (absDiff < 86400000) return 'in ' + Math.floor(absDiff / 3600000) + 'h';
          return 'in ' + Math.floor(absDiff / 86400000) + 'd';
        }
        if (diff < 60000) return 'just now';
        if (diff < 3600000) return Math.floor(diff / 60000) + 'm ago';
        if (diff < 86400000) return Math.floor(diff / 3600000) + 'h ago';
        return Math.floor(diff / 86400000) + 'd ago';
      } catch(e) { return 'never'; }
    },

    truncate(s, n) {
      if (!s) return '';
      if (s.length <= n) return s;
      return s.substring(0, n - 1) + '…';
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
