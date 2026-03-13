'use client';
export default function Error({ error, reset }) {
  return (
    <div className="error-state">
      ⚠ {error?.message || 'Something went wrong'}
      <button className="btn btn-ghost btn-sm" onClick={reset}>Retry</button>
    </div>
  );
}
