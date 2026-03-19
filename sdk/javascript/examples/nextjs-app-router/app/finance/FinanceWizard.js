'use client';

import { useState } from 'react';
import FinanceWizardStepBusiness from './wizard/FinanceWizardStepBusiness';
import FinanceWizardStepMoneySources from './wizard/FinanceWizardStepMoneySources';
import FinanceWizardStepCosts from './wizard/FinanceWizardStepCosts';
import FinanceWizardStepApprovals from './wizard/FinanceWizardStepApprovals';
import FinanceWizardStepFirstHelp from './wizard/FinanceWizardStepFirstHelp';
import FinanceWizardStepReview from './wizard/FinanceWizardStepReview';
import { buildFinancePayloadFromWizard, getFinanceReviewSummary } from './lib/finance-ui';

const STEPS = [
  'Business',
  'Revenue',
  'Costs',
  'Approvals',
  'First help',
  'Review',
];

const EMPTY_STATE = {
  businessMode: '',
  mainGoal: '',
  monthlyRevenue: null,
  cashOnHand: null,
  tracksInvoices: false,
  tracksSubscriptions: false,
  monthlyExpenses: null,
  tracksPayroll: false,
  tracksAdSpend: false,
  tracksServerCosts: false,
  tracksApiCosts: false,
  approvalRules: [],
  firstHelp: '',
};

function ProgressBar({ step, total }) {
  return (
    <div style={{ display: 'flex', gap: 4, marginBottom: 24 }}>
      {Array.from({ length: total }, (_, i) => (
        <div
          key={i}
          style={{
            flex: 1,
            height: 3,
            borderRadius: 2,
            background: i < step ? 'var(--accent)' : 'var(--border)',
            transition: 'background 0.2s',
          }}
        />
      ))}
    </div>
  );
}

export default function FinanceWizard({ open, onClose, onSaveProfile }) {
  const [step, setStep] = useState(0);
  const [form, setForm] = useState(EMPTY_STATE);
  const [creating, setCreating] = useState(false);

  if (!open) return null;

  function patch(fields) {
    setForm((prev) => ({ ...prev, ...fields }));
  }

  function toggleRule(key) {
    setForm((prev) => ({
      ...prev,
      approvalRules: prev.approvalRules.includes(key)
        ? prev.approvalRules.filter((r) => r !== key)
        : [...prev.approvalRules, key],
    }));
  }

  async function handleCreate() {
    setCreating(true);
    try {
      const payload = buildFinancePayloadFromWizard(form);
      await onSaveProfile(payload);
      setStep(0);
      setForm(EMPTY_STATE);
      onClose();
    } finally {
      setCreating(false);
    }
  }

  const reviewSummary = getFinanceReviewSummary(form);

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        zIndex: 1000,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'rgba(0,0,0,0.55)',
        padding: 20,
      }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
      data-cy="finance-wizard-overlay"
    >
      <div
        style={{
          width: '100%',
          maxWidth: 480,
          borderRadius: 14,
          background: 'var(--bg-card)',
          border: '1px solid var(--border)',
          boxShadow: '0 20px 60px rgba(0,0,0,0.35)',
          padding: '24px 26px 26px',
          maxHeight: '90vh',
          overflowY: 'auto',
        }}
        data-cy="finance-wizard-modal"
      >
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 18 }}>
          <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.07em' }}>
            Finance setup — {STEPS[step]}
          </div>
          <button
            className="btn btn-ghost btn-xs"
            onClick={onClose}
            aria-label="Close wizard"
            style={{ padding: '3px 7px' }}
          >
            ✕
          </button>
        </div>

        <ProgressBar step={step + 1} total={STEPS.length} />

        {step === 0 && (
          <FinanceWizardStepBusiness
            value={{ businessMode: form.businessMode, mainGoal: form.mainGoal }}
            onChange={patch}
            onNext={() => setStep(1)}
          />
        )}
        {step === 1 && (
          <FinanceWizardStepMoneySources
            value={{ monthlyRevenue: form.monthlyRevenue, cashOnHand: form.cashOnHand, tracksInvoices: form.tracksInvoices, tracksSubscriptions: form.tracksSubscriptions }}
            onChange={patch}
            onBack={() => setStep(0)}
            onNext={() => setStep(2)}
          />
        )}
        {step === 2 && (
          <FinanceWizardStepCosts
            value={{ monthlyExpenses: form.monthlyExpenses, tracksPayroll: form.tracksPayroll, tracksAdSpend: form.tracksAdSpend, tracksServerCosts: form.tracksServerCosts, tracksApiCosts: form.tracksApiCosts }}
            onChange={patch}
            onBack={() => setStep(1)}
            onNext={() => setStep(3)}
          />
        )}
        {step === 3 && (
          <FinanceWizardStepApprovals
            value={{ approvalRules: form.approvalRules }}
            onToggleRule={toggleRule}
            onBack={() => setStep(2)}
            onNext={() => setStep(4)}
          />
        )}
        {step === 4 && (
          <FinanceWizardStepFirstHelp
            value={form.firstHelp}
            onSelect={(v) => patch({ firstHelp: v })}
            onBack={() => setStep(3)}
            onNext={() => setStep(5)}
          />
        )}
        {step === 5 && (
          <FinanceWizardStepReview
            summary={reviewSummary}
            creating={creating}
            onBack={() => setStep(4)}
            onCreate={handleCreate}
          />
        )}
      </div>
    </div>
  );
}
