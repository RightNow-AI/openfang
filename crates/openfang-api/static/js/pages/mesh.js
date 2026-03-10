// Multi-Agent Mesh page component
function meshPage() {
  return {
    loading: false,
    peers: [],
    a2aAgents: [],
    routeLog: [],
    connectAddr: '',
    connecting: false,
    connectError: null,

    async loadAll() {
      this.loading = true;
      try {
        await Promise.all([
          this.loadPeers(),
          this.loadA2aAgents(),
          this.loadRouteLog(),
        ]);
      } finally {
        this.loading = false;
      }
    },

    async loadPeers() {
      try {
        const resp = await fetch('/api/mesh/peers');
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        const data = await resp.json();
        this.peers = data.peers || data || [];
      } catch (e) {
        this.peers = [];
      }
    },

    async loadA2aAgents() {
      try {
        const resp = await fetch('/api/a2a/agents');
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        const data = await resp.json();
        this.a2aAgents = data.agents || data || [];
      } catch (e) {
        this.a2aAgents = [];
      }
    },

    async loadRouteLog() {
      try {
        const resp = await fetch('/api/mesh/route-log');
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        const data = await resp.json();
        this.routeLog = data.entries || data || [];
      } catch (e) {
        this.routeLog = [];
      }
    },

    async connectPeer() {
      if (!this.connectAddr.trim()) return;
      this.connecting = true;
      this.connectError = null;
      try {
        const resp = await fetch('/api/mesh/connect', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ address: this.connectAddr.trim() }),
        });
        if (!resp.ok) {
          const err = await resp.json().catch(() => ({}));
          throw new Error(err.error || `HTTP ${resp.status}`);
        }
        this.connectAddr = '';
        window.showToast?.('Peer connection initiated', 'success');
        await this.loadPeers();
      } catch (e) {
        this.connectError = e.message;
      } finally {
        this.connecting = false;
      }
    },

    async disconnectPeer(peerId) {
      try {
        const resp = await fetch(`/api/mesh/peers/${encodeURIComponent(peerId)}`, {
          method: 'DELETE',
        });
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        window.showToast?.('Peer disconnected', 'success');
        await this.loadPeers();
      } catch (e) {
        window.showToast?.(`Disconnect failed: ${e.message}`, 'error');
      }
    },
  };
}
