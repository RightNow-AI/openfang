'use client';

import { useState, useEffect, useCallback } from 'react';
import FinanceHeader from './FinanceHeader';
import FinanceTabs from './FinanceTabs';
import FinanceViewToggle from './FinanceViewToggle';
import FinanceWizard from './FinanceWizard';
import FinanceDetailDrawer from './FinanceDetailDrawer';
import RecommendedFinanceTab from './RecommendedFinanceTab';
import FinanceOverviewTab from './FinanceOverviewTab';
import FinanceTemplatesTab from './FinanceTemplatesTab';
import AdvancedFinanceTab from './AdvancedFinanceTab';
import InvestmentFinanceTab from './InvestmentFinanceTab';
import { emptyFinanceSummary } from './lib/finance-ui';

export default function FinancePage({ initialTab, initialView, autoOpenWizard }) {
  const [activeTab, setActiveTab] = useState(initialTab || 'recommended');
  const [view, setView] = useState(initialView || 'simple');
  const [summary, setSummary] = useState(null);
  const [loadingSummary, setLoadingSummary] = useState(true);
  const [summaryError, setSummaryError] = useState(null);
  const [wizardOpen, setWizardOpen] = useState(autoOpenWizard ?? false);
  const [applyingTemplateId, setApplyingTemplateId] = useState(null);
  const [refreshing, setRefreshing] = useState(false);
  const [drawerItemId, setDrawerItemId] = useState(null);
  const [drawerDetail, setDrawerDetail] = useState(null);
  const [drawerLoading, setDrawerLoading] = useState(false);
  const [drawerError, setDrawerError] = useState(null);

  const loadSummary = useCallback(async (silent = false) => {
    if (!silent) setLoadingSummary(true);
    setSummaryError(null);
    try {
      const res = await fetch('/api/finance/summary');
      if (res.ok) {
        const data = await res.json();
        setSummary(data);
      } else if (res.status === 404 || res.status === 405) {
        setSummary(emptyFinanceSummary());
      } else {
        setSummaryError('Failed to load finance data.');
        setSummary(emptyFinanceSummary());
      }
    } catch {
      setSummaryError('Could not connect to backend.');
      setSummary(emptyFinanceSummary());
    } finally {
      setLoadingSummary(false);
      setRefreshing(false);
    }
  }, []);

  useEffect(() => { loadSummary(); }, [loadSummary]);

  async function handleRefresh() {
    setRefreshing(true);
    await loadSummary(true);
  }

  async function handleSaveProfile(payload) {
    const method = summary?.profile ? 'PUT' : 'POST';
    await fetch('/api/finance/profile', {
      method,
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    await loadSummary(true);
    setActiveTab('overview');
  }

  async function handleApplyTemplate(templateId) {
    setApplyingTemplateId(templateId);
    try {
      await fetch('/api/finance/templates/apply', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ templateId }),
      });
      await loadSummary(true);
      setActiveTab('overview');
    } finally {
      setApplyingTemplateId(null);
    }
  }

  async function handleOpenDetail(id) {
    setDrawerItemId(id);
    setDrawerDetail(null);
    setDrawerLoading(true);
    setDrawerError(null);
    try {
      const res = await fetch(`/api/finance/items/${id}`);
      if (res.ok) {
        const data = await res.json();
        setDrawerDetail(data.detail ?? data);
      } else {
        setDrawerDetail(null);
      }
    } catch {
      setDrawerError('Failed to load detail.');
    } finally {
      setDrawerLoading(false);
    }
  }

  async function handleApproveAction(actionId) {
    try {
      await fetch(`/api/finance/actions/${actionId}/approve`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ actionId }),
      });
      setDrawerItemId(null);
      await loadSummary(true);
    } catch {
      // silently ignore — approval will retry on refresh
    }
  }

  const hasProfile = Boolean(summary?.profile);

  return (
    <div style={{ maxWidth: 1100, margin: '0 auto', padding: '24px 20px' }} data-cy="finance-page">
      <FinanceHeader
        hasProfile={hasProfile}
        onOpenWizard={() => setWizardOpen(true)}
        onSwitchTab={setActiveTab}
        onRefresh={handleRefresh}
        refreshing={refreshing}
      />

      {summaryError && (
        <div className="badge badge-warn" style={{ marginBottom: 16, padding: '8px 12px', fontSize: 12 }}>
          {summaryError}
        </div>
      )}

      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 4 }}>
        <FinanceTabs activeTab={activeTab} onChange={setActiveTab} />
        {activeTab === 'overview' && (
          <FinanceViewToggle view={view} onChange={setView} />
        )}
      </div>

      {loadingSummary ? (
        <div style={{ textAlign: 'center', padding: '48px 0' }}>
          <span className="spinner" />
        </div>
      ) : (
        <>
          {activeTab === 'recommended' && (
            <RecommendedFinanceTab
              onApplyTemplate={handleApplyTemplate}
              onOpenWizard={() => setWizardOpen(true)}
              applyingTemplateId={applyingTemplateId}
            />
          )}
          {activeTab === 'overview' && (
            <FinanceOverviewTab
              summary={summary}
              view={view}
              onOpenDetail={handleOpenDetail}
            />
          )}
          {activeTab === 'investment' && (
            <InvestmentFinanceTab />
          )}
          {activeTab === 'templates' && (
            <FinanceTemplatesTab
              onApplyTemplate={handleApplyTemplate}
              applyingTemplateId={applyingTemplateId}
            />
          )}
          {activeTab === 'advanced' && (
            <AdvancedFinanceTab onOpenWizard={() => setWizardOpen(true)} />
          )}
        </>
      )}

      <FinanceWizard
        open={wizardOpen}
        onClose={() => setWizardOpen(false)}
        onSaveProfile={handleSaveProfile}
      />

      <FinanceDetailDrawer
        itemId={drawerItemId}
        detail={drawerDetail}
        loading={drawerLoading}
        error={drawerError}
        onApproveAction={drawerItemId === 'approvals' ? handleApproveAction : null}
        onClose={() => setDrawerItemId(null)}
      />
    </div>
  );
}
