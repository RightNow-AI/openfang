'use client';

import { useState, useCallback } from 'react';
import InvestmentsWizardStepScope from './wizard/InvestmentsWizardStepScope';
import InvestmentsWizardStepSignals from './wizard/InvestmentsWizardStepSignals';
import InvestmentsWizardStepPatterns from './wizard/InvestmentsWizardStepPatterns';
import InvestmentsWizardStepApprovals from './wizard/InvestmentsWizardStepApprovals';
import InvestmentsWizardStepDataSources from './wizard/InvestmentsWizardStepDataSources';
import InvestmentsWizardStepReview from './wizard/InvestmentsWizardStepReview';
import { buildInvestmentsPayload, getInvestmentsReviewSummary } from './lib/investments-ui';

const EMPTY_STATE = {
  watchScope: '',
  symbols: '',
  timeHorizon: '',
  riskComfort: '',
  signals: [],
  patterns: [],
  approvalRules: [],
  providers: [],
  inputMethod: 'manual',
  apiKeyName: '',
  defaultMarket: '',
  pollingCadence: 'daily',
  fallbackSource: '',
};

const STEPS = ['Scope', 'Signals', 'Patterns', 'Approvals', 'Data', 'Review'];

export default function InvestmentsWizard({ open, onClose, onSaveSetup }) {
  const [step, setStep] = useState(0);
  const [form, setForm] = useState(EMPTY_STATE);
  const [creating, setCreating] = useState(false);
  const [testingConnection, setTestingConnection] = useState(false);
  const [testResult, setTestResult] = useState(null);

  const reset = useCallback(() => {
    setStep(0);
    setForm(EMPTY_STATE);
    setCreating(false);
    setTestResult(null);
  }, []);

  function handleClose() {
    reset();
    onClose();
  }

  function toggleArr(field, val) {
    setForm((f) => {
      const arr = f[field] || [];
      return { ...f, [field]: arr.includes(val) ? arr.filter((x) => x !== val) : [...arr, val] };
    });
  }

  async function handleCreate() {
    setCreating(true);
    try {
      const payload = buildInvestmentsPayload(form);
      await onSaveSetup(payload);
      handleClose();
    } finally {
      setCreating(false);
    }
  }

  async function handleTestConnection() {
    setTestingConnection(true);
    setTestResult(null);
    try {
      const res = await fetch('/api/investments/providers/test', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ providers: form.providers, apiKeyName: form.apiKeyName }),
      });
      const data = await res.json();
      setTestResult({ ok: data.ok, message: data.message || (data.ok ? 'Connection successful.' : 'Connection failed.') });
    } catch {
      setTestResult({ ok: false, message: 'Could not reach server.' });
    } finally {
      setTestingConnection(false);
    }
  }

  if (!open) return null;

  const progress = ((step + 1) / STEPS.length) * 100;
  const summary = getInvestmentsReviewSummary(form);

  return (
    <div
      style={{
        position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.55)', zIndex: 9999,
        display: 'flex', alignItems: 'center', justifyContent: 'center', padding: '16px',
      }}
      onClick={(e) => { if (e.target === e.currentTarget) handleClose(); }}
    >
      <div
        style={{
          background: 'var(--background)',
          border: '1.5px solid var(--border)',
          borderRadius: 14,
          width: '100%',
          maxWidth: 560,
          maxHeight: '90vh',
          overflow: 'auto',
          padding: '28px 28px 24px',
          position: 'relative',
        }}
      >
        {/* Header */}
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 18 }}>
          <div>
            <div style={{ fontSize: 11, color: 'var(--text-dim)', marginBottom: 4, textTransform: 'uppercase', letterSpacing: 0.5 }}>
              Step {step + 1} of {STEPS.length} — {STEPS[step]}
            </div>
            {/* Progress bar */}
            <div style={{ width: 220, height: 3, background: 'var(--border)', borderRadius: 2 }}>
              <div style={{ width: `${progress}%`, height: '100%', background: 'var(--accent)', borderRadius: 2, transition: 'width 0.25s' }} />
            </div>
          </div>
          <button
            className="btn btn-ghost btn-xs"
            onClick={handleClose}
            style={{ fontSize: 16, lineHeight: 1 }}
          >
            ✕
          </button>
        </div>

        {/* Steps */}
        {step === 0 && (
          <InvestmentsWizardStepScope
            value={{ watchScope: form.watchScope, symbols: form.symbols, timeHorizon: form.timeHorizon, riskComfort: form.riskComfort }}
            onChange={(v) => setForm((f) => ({ ...f, ...v }))}
            onNext={() => setStep(1)}
          />
        )}
        {step === 1 && (
          <InvestmentsWizardStepSignals
            value={form.signals}
            onToggle={(sig) => toggleArr('signals', sig)}
            onBack={() => setStep(0)}
            onNext={() => setStep(2)}
          />
        )}
        {step === 2 && (
          <InvestmentsWizardStepPatterns
            value={form.patterns}
            onToggle={(pat) => toggleArr('patterns', pat)}
            onBack={() => setStep(1)}
            onNext={() => setStep(3)}
          />
        )}
        {step === 3 && (
          <InvestmentsWizardStepApprovals
            value={form.approvalRules}
            onToggle={(rule) => toggleArr('approvalRules', rule)}
            onBack={() => setStep(2)}
            onNext={() => setStep(4)}
          />
        )}
        {step === 4 && (
          <InvestmentsWizardStepDataSources
            value={{ inputMethod: form.inputMethod, providers: form.providers, apiKeyName: form.apiKeyName, defaultMarket: form.defaultMarket, pollingCadence: form.pollingCadence, fallbackSource: form.fallbackSource }}
            onChange={(v) => setForm((f) => ({ ...f, ...v }))}
            onTestConnection={handleTestConnection}
            testingConnection={testingConnection}
            testResult={testResult}
            onBack={() => setStep(3)}
            onNext={() => setStep(5)}
          />
        )}
        {step === 5 && (
          <InvestmentsWizardStepReview
            summary={summary}
            creating={creating}
            onBack={() => setStep(4)}
            onCreate={handleCreate}
          />
        )}
      </div>
    </div>
  );
}
