'use client'

import { useState } from 'react'

const STEPS = [
  { id: 1, label: 'Introduction' },
  { id: 2, label: 'Credentials' },
  { id: 3, label: 'Test' },
  { id: 4, label: 'Done' },
]

export default function SetupWizard({ platform }) {
  const [step, setStep] = useState(1)
  const [fields, setFields] = useState({})
  const [testing, setTesting] = useState(false)
  const [testPassed, setTestPassed] = useState(null)

  function update(key, val) {
    setFields((f) => ({ ...f, [key]: val }))
  }

  async function handleTest() {
    setTesting(true)
    setTestPassed(null)
    // Wire up real test call here: POST /api/connect/[platform]/test
    await new Promise((r) => setTimeout(r, 1200))
    setTestPassed(true)
    setTesting(false)
  }

  const pct = Math.round(((step - 1) / (STEPS.length - 1)) * 100)

  return (
    <div className="card mx-auto max-w-lg">
      {/* Progress header */}
      <div className="border-b border-gray-50 px-6 py-5">
        <div className="mb-3 flex items-center justify-between">
          <p className="text-sm font-semibold text-gray-900">
            Set up {platform?.charAt(0).toUpperCase() + platform?.slice(1)}
          </p>
          <p className="text-xs text-gray-400">
            Step {step} of {STEPS.length}
          </p>
        </div>
        <div className="h-1.5 w-full overflow-hidden rounded-full bg-gray-100">
          <div
            className="h-full rounded-full bg-accent-600 transition-all duration-300"
            style={{ width: `${pct}%` }}
          />
        </div>
        <div className="mt-2 flex justify-between">
          {STEPS.map((s) => (
            <span
              key={s.id}
              className={[
                'text-[10px] font-medium',
                s.id === step ? 'text-accent-700' : s.id < step ? 'text-gray-400' : 'text-gray-300',
              ].join(' ')}
            >
              {s.label}
            </span>
          ))}
        </div>
      </div>

      {/* Step content */}
      <div className="px-6 py-6">
        {step === 1 && (
          <div className="space-y-3">
            <p className="text-sm text-gray-600">
              You are about to connect <strong>{platform}</strong> to OpenFang.
              Your assistant will be able to read and send messages on your behalf.
            </p>
            <ul className="space-y-1.5 text-xs text-gray-500">
              {['Read incoming messages', 'Send replies', 'Access contact info'].map((perm) => (
                <li key={perm} className="flex items-center gap-2">
                  <svg className="h-3.5 w-3.5 text-emerald-500" fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
                  </svg>
                  {perm}
                </li>
              ))}
            </ul>
          </div>
        )}

        {step === 2 && (
          <div className="space-y-4">
            <div>
              <label className="mb-1.5 block text-xs font-medium text-gray-700">API key</label>
              <input
                className="input-field"
                type="password"
                placeholder="Paste your API key here"
                value={fields.apiKey ?? ''}
                onChange={(e) => update('apiKey', e.target.value)}
              />
            </div>
            <div>
              <label className="mb-1.5 block text-xs font-medium text-gray-700">Webhook URL (optional)</label>
              <input
                className="input-field"
                type="text"
                placeholder="https://…"
                value={fields.webhook ?? ''}
                onChange={(e) => update('webhook', e.target.value)}
              />
            </div>
          </div>
        )}

        {step === 3 && (
          <div className="space-y-4">
            <p className="text-sm text-gray-600">
              Send a test message to verify the connection is working.
            </p>
            {testPassed === true && (
              <div className="flex items-center gap-2 rounded-xl bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
                </svg>
                Connection verified
              </div>
            )}
            {testPassed === false && (
              <div className="flex items-center gap-2 rounded-xl bg-red-50 px-4 py-3 text-sm text-red-700">
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
                </svg>
                Test failed. Check your credentials.
              </div>
            )}
            <button
              onClick={handleTest}
              disabled={testing}
              className="btn-secondary w-full"
            >
              {testing ? 'Testing…' : 'Send test message'}
            </button>
          </div>
        )}

        {step === 4 && (
          <div className="space-y-3 text-center">
            <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-2xl bg-emerald-100">
              <svg className="h-6 w-6 text-emerald-600" fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
              </svg>
            </div>
            <p className="text-sm font-semibold text-gray-900">
              {platform?.charAt(0).toUpperCase() + platform?.slice(1)} is connected
            </p>
            <p className="text-xs text-gray-500">
              Your assistant will now respond to messages from this platform.
            </p>
          </div>
        )}
      </div>

      {/* Footer buttons */}
      <div className="flex items-center justify-between border-t border-gray-50 px-6 py-4">
        {step > 1 ? (
          <button onClick={() => setStep((s) => s - 1)} className="btn-ghost">
            Back
          </button>
        ) : (
          <div />
        )}
        {step < STEPS.length ? (
          <button
            onClick={() => setStep((s) => s + 1)}
            disabled={step === 3 && !testPassed}
            className="btn-primary"
          >
            Continue
          </button>
        ) : (
          <a href="/connect" className="btn-primary">
            Done
          </a>
        )}
      </div>
    </div>
  )
}
