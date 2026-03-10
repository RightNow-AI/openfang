// FangHub Marketplace page component
function fanghubPage() {
  return {
    query: '',
    results: [],
    loading: false,
    error: null,
    installing: null,

    async search() {
      this.loading = true;
      this.error = null;
      try {
        const url = this.query
          ? `/api/fanghub/search?q=${encodeURIComponent(this.query)}`
          : '/api/fanghub/search';
        const resp = await fetch(url);
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        const data = await resp.json();
        this.results = data.packages || data.results || data || [];
      } catch (e) {
        this.error = e.message || 'Failed to load FangHub packages';
        this.results = [];
      } finally {
        this.loading = false;
      }
    },

    async install(handId) {
      this.installing = handId;
      try {
        const resp = await fetch('/api/fanghub/install', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ hand_id: handId }),
        });
        if (!resp.ok) {
          const err = await resp.json().catch(() => ({}));
          throw new Error(err.error || `HTTP ${resp.status}`);
        }
        window.showToast?.(`Hand "${handId}" installed successfully`, 'success');
        await this.search();
      } catch (e) {
        window.showToast?.(`Install failed: ${e.message}`, 'error');
      } finally {
        this.installing = null;
      }
    },
  };
}
