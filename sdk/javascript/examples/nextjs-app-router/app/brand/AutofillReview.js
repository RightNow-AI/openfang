'use client';

/**
 * AutofillReview — overlay panel shown after the autofill API returns suggestions
 * gleaned from the user's website URL.
 *
 * Props:
 *   data      — { patch: Record<string, any>, found: string[], missing: string[], error?: string }
 *   onAccept  — (patch) => void
 *   onDismiss — () => void
 */

// Human-readable labels for profile field keys
const FIELD_LABELS = {
  business_name:    'Business name',
  industry:         'Industry',
  business_model:   'Business type',
  location:         'Location',
  primary_offer:    'Primary offer',
  main_goal_90_days:'90-day goal',
  ideal_customer:   'Ideal customer',
  brand_promise:    'Brand promise',
  core_offer:       'Core offer',
  pricing_model:    'Pricing model',
  primary_cta:      'Call to action',
};

function label(key) {
  return FIELD_LABELS[key] ?? key.replace(/_/g, ' ');
}

export default function AutofillReview({ data, onAccept, onDismiss }) {
  const isError = !!data.error;
  const found   = data.found   ?? Object.keys(data.patch ?? {});
  const missing = data.missing ?? [];
  const patch   = data.patch   ?? {};

  return (
    <>
      {/* Backdrop */}
      <div
        onClick={onDismiss}
        style={{
          position: 'fixed', inset: 0,
          background: 'rgba(0,0,0,0.45)', zIndex: 200,
        }}
      />

      {/* Panel */}
      <div style={{
        position: 'fixed', top: '50%', left: '50%',
        transform: 'translate(-50%, -50%)',
        zIndex: 201, width: 480, maxWidth: 'calc(100vw - 32px)',
        maxHeight: '80vh', overflowY: 'auto',
        background: 'var(--bg-elevated)',
        border: '1px solid var(--border)',
        borderRadius: 12,
        boxShadow: '0 24px 48px rgba(0,0,0,0.35)',
        padding: 24,
      }}>
        {isError ? (
          <>
            <div style={{ fontSize: 16, fontWeight: 700, color: 'var(--text)', marginBottom: 8 }}>
              Couldn&apos;t read the website
            </div>
            <div style={{ fontSize: 13, color: 'var(--text-muted)', marginBottom: 20 }}>
              {data.error}
            </div>
            <button
              onClick={onDismiss}
              style={{
                padding: '8px 18px', borderRadius: 7, border: '1px solid var(--border)',
                background: 'transparent', color: 'var(--text)', cursor: 'pointer', fontSize: 13,
              }}
            >Got it</button>
          </>
        ) : (
          <>
            <div style={{ fontSize: 16, fontWeight: 700, color: 'var(--text)', marginBottom: 4 }}>
              We found a few details from your website
            </div>
            <div style={{ fontSize: 13, color: 'var(--text-muted)', marginBottom: 20 }}>
              Would you like to use these? You can edit any field after applying.
            </div>

            {/* What was found */}
            {found.length > 0 && (
              <div style={{ marginBottom: 16 }}>
                <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--success)', textTransform: 'uppercase', letterSpacing: '0.5px', marginBottom: 8 }}>
                  What we found
                </div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                  {found.map(key => (
                    <div key={key} style={{
                      padding: '8px 12px', borderRadius: 7,
                      background: 'var(--surface2)', border: '1px solid var(--border)',
                    }}>
                      <div style={{ fontSize: 10, fontWeight: 700, color: 'var(--text-muted)', textTransform: 'uppercase', marginBottom: 2 }}>
                        {label(key)}
                      </div>
                      <div style={{ fontSize: 13, color: 'var(--text)' }}>
                        {String(patch[key]).slice(0, 200)}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* What's still missing */}
            {missing.length > 0 && (
              <div style={{ marginBottom: 20 }}>
                <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.5px', marginBottom: 8 }}>
                  Still needs your input
                </div>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 5 }}>
                  {missing.map(key => (
                    <span key={key} style={{
                      padding: '3px 8px', borderRadius: 5, fontSize: 11,
                      background: 'var(--surface3)', color: 'var(--text-dim)',
                      border: '1px solid var(--border)',
                    }}>{label(key)}</span>
                  ))}
                </div>
              </div>
            )}

            {/* Actions */}
            <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
              <button
                onClick={() => onAccept(patch)}
                style={{
                  flex: 1, minWidth: 120, padding: '9px 18px', borderRadius: 7, border: 'none',
                  background: 'var(--accent)', color: '#fff', cursor: 'pointer',
                  fontSize: 13, fontWeight: 600, boxShadow: 'var(--shadow-accent)',
                }}
              >
                Use these answers
              </button>
              <button
                onClick={() => onAccept(patch)}
                style={{
                  flex: 1, minWidth: 120, padding: '9px 18px', borderRadius: 7,
                  border: '1px solid var(--border)', background: 'var(--surface2)',
                  color: 'var(--text)', cursor: 'pointer', fontSize: 13,
                }}
              >
                Apply &amp; edit
              </button>
              <button
                onClick={onDismiss}
                style={{
                  padding: '9px 14px', borderRadius: 7, border: '1px solid var(--border)',
                  background: 'transparent', color: 'var(--text-muted)', cursor: 'pointer', fontSize: 13,
                }}
              >
                Cancel
              </button>
            </div>
          </>
        )}
      </div>
    </>
  );
}
