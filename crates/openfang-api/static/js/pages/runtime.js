// Runtime page — system overview and provider status
document.addEventListener('alpine:init', function() {
  Alpine.data('runtimePage', function() {
    return {
      loading: true,
      uptime: '-',
      agentCount: 0,
      version: '-',
      defaultModel: '-',
      platform: '-',
      arch: '-',
      apiListen: '-',
      homeDir: '-',
      logLevel: '-',
      networkEnabled: false,
      providers: [],
      modelProfiles: [],
      activeProfileId: '',
      selectedProfileId: '',
      restartVllmOnApply: false,
      modelSwitchStatus: '',
      modelSwitchBusy: false,

      async loadData() {
        this.loading = true;
        try {
          var results = await Promise.all([
            OMTAEAPI.get('/api/status'),
            OMTAEAPI.get('/api/version'),
            OMTAEAPI.get('/api/providers'),
            OMTAEAPI.get('/api/agents'),
            OMTAEAPI.get('/api/models/profiles').catch(function() { return { profiles: [], active_profile: '' }; }),
            OMTAEAPI.get('/api/models/active').catch(function() { return {}; })
          ]);
          var status = results[0];
          var ver = results[1];
          var prov = results[2];
          var agents = results[3];
          var prof = results[4] || {};
          var active = results[5] || {};

          this.version = ver.version || '-';
          this.platform = ver.platform || '-';
          this.arch = ver.arch || '-';
          this.agentCount = Array.isArray(agents) ? agents.length : 0;
          this.defaultModel = status.default_model || '-';
          this.apiListen = status.api_listen || status.listen || '-';
          this.homeDir = status.home_dir || '-';
          this.logLevel = status.log_level || '-';
          this.networkEnabled = !!status.network_enabled;

          // Compute uptime from uptime_seconds
          var diff = status.uptime_seconds || 0;
          if (diff < 60) this.uptime = diff + 's';
          else if (diff < 3600) this.uptime = Math.floor(diff / 60) + 'm ' + (diff % 60) + 's';
          else if (diff < 86400) this.uptime = Math.floor(diff / 3600) + 'h ' + Math.floor((diff % 3600) / 60) + 'm';
          else this.uptime = Math.floor(diff / 86400) + 'd ' + Math.floor((diff % 86400) / 3600) + 'h';

          this.providers = (prov.providers || []).filter(function(p) {
            return p.auth_status === 'Configured' || p.reachable || p.is_local;
          });

          this.modelProfiles = prof.profiles || [];
          this.activeProfileId = prof.active_profile || active.active_profile || '';
          if (!this.selectedProfileId && this.activeProfileId) {
            this.selectedProfileId = this.activeProfileId;
          } else if (!this.selectedProfileId && this.modelProfiles.length) {
            this.selectedProfileId = this.modelProfiles[0].id;
          }
        } catch(e) {
          console.error('Runtime load error:', e);
        }
        this.loading = false;
      },

      async applyModelProfile() {
        if (!this.selectedProfileId) return;
        this.modelSwitchBusy = true;
        this.modelSwitchStatus = '';
        try {
          var res = await OMTAEAPI.put('/api/models/active', {
            profile_id: this.selectedProfileId,
            restart_vllm: !!this.restartVllmOnApply
          });
          this.activeProfileId = this.selectedProfileId;
          this.defaultModel = res.omtae_model_id || this.defaultModel;
          this.modelSwitchStatus = res.message || 'Applied.';
          if (res.message && res.message.indexOf('omtae-model use') >= 0) {
            this.modelSwitchStatus += ' Full GPU swap: omtae-model use ' + this.selectedProfileId;
          }
        } catch (e) {
          this.modelSwitchStatus = (e && e.message) ? e.message : 'Switch failed';
        }
        this.modelSwitchBusy = false;
      }
    };
  });
});
