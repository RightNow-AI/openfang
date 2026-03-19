'use client';

import { useState, useEffect, useCallback } from 'react';
import InvestmentsHeader from './InvestmentsHeader';
import InvestmentsTabs from './InvestmentsTabs';
import InvestmentsViewToggle from './InvestmentsViewToggle';
import InvestmentsWizard from './InvestmentsWizard';
import InvestmentDetailDrawer from './InvestmentDetailDrawer';
import ThesisDrawer from './ThesisDrawer';
import AlertDrawer from './AlertDrawer';
import RecommendedInvestmentsTab from './RecommendedInvestmentsTab';
import WatchlistTab from './WatchlistTab';
import ResearchTab from './ResearchTab';
import PortfolioTab from './PortfolioTab';
import AlertsTab from './AlertsTab';
import AdvancedInvestmentsTab from './AdvancedInvestmentsTab';

export default function InvestmentsPage({ defaultTab = 'recommended', defaultView = 'simple', defaultWizard = false }) {
  const [activeTab, setActiveTab] = useState(defaultTab);
  const [view, setView] = useState(defaultView);
  const [wizardOpen, setWizardOpen] = useState(defaultWizard);

  const [watchlist, setWatchlist] = useState([]);
  const [research, setResearch] = useState([]);
  const [portfolio, setPortfolio] = useState([]);
  const [alerts, setAlerts] = useState([]);
  const [catalysts, setCatalysts] = useState([]);

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [refreshing, setRefreshing] = useState(false);
  const [applyingTemplateId, setApplyingTemplateId] = useState(null);

  // Drawer state
  const [detailItem, setDetailItem] = useState(null);
  const [thesisItem, setThesisItem] = useState(null);
  const [alertItem, setAlertItem] = useState(null);

  const fetchAll = useCallback(async (silent = false) => {
    if (!silent) setLoading(true);
    setError(null);
    try {
      const [wl, res, port, alts] = await Promise.all([
        fetch('/api/investments/watchlist').then((r) => r.json()),
        fetch('/api/investments/research').then((r) => r.json()),
        fetch('/api/investments/portfolio').then((r) => r.json()),
        fetch('/api/investments/alerts').then((r) => r.json()),
      ]);
      setWatchlist(wl.items || []);
      setResearch(res.items || []);
      setPortfolio(port.items || []);
      setAlerts(alts.items || []);
      setCatalysts(res.catalysts || []);
    } catch {
      setError('Could not load investment data.');
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, []);

  useEffect(() => { fetchAll(); }, [fetchAll]);

  function handleRefresh() {
    setRefreshing(true);
    fetchAll(true);
  }

  async function handleSaveSetup(payload) {
    await fetch('/api/investments/setup', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    fetchAll(true);
  }

  async function handleApplyTemplate(templateId) {
    setApplyingTemplateId(templateId);
    try {
      await fetch('/api/investments/templates/apply', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ templateId }),
      });
      fetchAll(true);
      setActiveTab('watchlist');
    } finally {
      setApplyingTemplateId(null);
    }
  }

  async function handleAddSymbol(symbol) {
    await fetch('/api/investments/watchlist', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ symbol }),
    });
    fetchAll(true);
  }

  async function handleApproveAlert(alertId) {
    await fetch(`/api/investments/alerts/${alertId}/approve`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ action: 'approve' }),
    });
    fetchAll(true);
    setAlertItem(null);
  }

  function handleOpenDetail(id) {
    const item = watchlist.find((w) => w.id === id);
    if (item) setDetailItem(item);
  }

  function handleOpenThesis(id) {
    const item = research.find((r) => r.id === id) || watchlist.find((w) => w.id === id);
    if (item) setThesisItem(item);
  }

  function handleOpenAlert(id) {
    const item = alerts.find((a) => a.id === id);
    if (item) setAlertItem(item);
  }

  const alertCount = alerts.filter((a) => a.approval_required && a.approval_status === 'pending').length;

  return (
    <div style={{ maxWidth: 1100, margin: '0 auto', padding: '28px 20px' }}>
      <InvestmentsHeader
        watchCount={watchlist.length}
        alertCount={alertCount}
        onRefresh={handleRefresh}
        onOpenWizard={() => setWizardOpen(true)}
        onOpenTemplates={() => setActiveTab('recommended')}
        refreshing={refreshing}
      />

      <InvestmentsTabs
        activeTab={activeTab}
        onChange={setActiveTab}
        alertCount={alertCount}
      />

      {activeTab !== 'recommended' && activeTab !== 'advanced' && (
        <InvestmentsViewToggle view={view} onChange={setView} />
      )}

      {loading ? (
        <div style={{ display: 'flex', justifyContent: 'center', padding: '60px 0' }}>
          <span className="spinner" style={{ width: 24, height: 24 }} />
        </div>
      ) : error ? (
        <div className="card" style={{ color: '#ef4444', fontSize: 13, padding: '16px 18px' }}>
          {error}
        </div>
      ) : (
        <>
          {activeTab === 'recommended' && (
            <RecommendedInvestmentsTab
              applyingTemplateId={applyingTemplateId}
              onApplyTemplate={handleApplyTemplate}
              onOpenWizard={() => setWizardOpen(true)}
            />
          )}
          {activeTab === 'watchlist' && (
            <WatchlistTab
              items={watchlist}
              view={view}
              onOpenDetail={handleOpenDetail}
              onAddSymbol={handleAddSymbol}
            />
          )}
          {activeTab === 'research' && (
            <ResearchTab
              research={research}
              catalysts={catalysts}
              onOpenThesis={handleOpenThesis}
              onRefreshResearch={handleRefresh}
              refreshing={refreshing}
            />
          )}
          {activeTab === 'portfolio' && (
            <PortfolioTab
              positions={portfolio}
              onOpenPosition={(id) => {
                const p = portfolio.find((x) => x.id === id);
                if (p) setThesisItem(p);
              }}
            />
          )}
          {activeTab === 'alerts' && (
            <AlertsTab
              alerts={alerts}
              onOpenAlert={handleOpenAlert}
              onApproveAlert={handleApproveAlert}
            />
          )}
          {activeTab === 'advanced' && (
            <AdvancedInvestmentsTab onOpenWizard={() => setWizardOpen(true)} />
          )}
        </>
      )}

      <InvestmentsWizard
        open={wizardOpen}
        onClose={() => setWizardOpen(false)}
        onSaveSetup={handleSaveSetup}
      />

      {detailItem && (
        <InvestmentDetailDrawer
          item={detailItem}
          onUpdateThesis={(id) => { setDetailItem(null); handleOpenThesis(id); }}
          onArchive={() => { setDetailItem(null); fetchAll(true); }}
          onClose={() => setDetailItem(null)}
        />
      )}

      {thesisItem && (
        <ThesisDrawer
          thesis={thesisItem}
          onMarkIntact={() => { setThesisItem(null); fetchAll(true); }}
          onMarkWeakened={() => { setThesisItem(null); fetchAll(true); }}
          onMarkBroken={() => { setThesisItem(null); fetchAll(true); }}
          onRequestAction={() => setThesisItem(null)}
          onClose={() => setThesisItem(null)}
        />
      )}

      {alertItem && (
        <AlertDrawer
          alert={alertItem}
          onApprove={handleApproveAlert}
          onReject={() => { setAlertItem(null); fetchAll(true); }}
          onRequestChanges={() => setAlertItem(null)}
          onClose={() => setAlertItem(null)}
        />
      )}
    </div>
  );
}
