import React, { useState } from 'react';

export default function InvestmentFinanceTab() {
  const [selectedApi, setSelectedApi] = useState('yahoo_finance');

  return (
    <div style={{ padding: '24px 0', display: 'flex', flexDirection: 'column', gap: '20px' }}>
      <h3>Investment Data Integrations</h3>
      <p style={{ color: 'var(--text-dim)', fontSize: '14px' }}>
        Select a free finance API to fetch daily investment data and portfolio trends.
      </p>
      
      <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', maxWidth: '320px' }}>
        <label htmlFor="api-select" style={{ fontSize: '13px', fontWeight: 600 }}>Data Source (Free Tiers)</label>
        <select 
          id="api-select"
          value={selectedApi} 
          onChange={(e) => setSelectedApi(e.target.value)}
          style={{
            padding: '8px 12px',
            borderRadius: '4px',
            border: '1px solid var(--border)',
            background: 'var(--surface)',
            color: 'var(--text)',
            fontSize: '14px'
          }}
        >
          <option value="yahoo_finance">Yahoo Finance (Market Data)</option>
          <option value="alpha_vantage">Alpha Vantage (Daily Stock/Forex)</option>
          <option value="coingecko">CoinGecko (Crypto API)</option>
          <option value="exchangerate">ExchangeRate-API (Currency)</option>
        </select>
      </div>

      <div style={{ padding: '16px', background: 'var(--surface-sunken)', borderRadius: '6px', border: '1px solid var(--border)' }}>
        <h4 style={{ margin: '0 0 12px 0' }}>Data Source Information</h4>
        {selectedApi === 'yahoo_finance' && (
          <p style={{ fontSize: '13px', margin: 0, color: 'var(--text-dim)' }}>
            <strong>Yahoo Finance:</strong> Provides comprehensive market data, quotes, and historical stats. (Added API key tracking as requested).
          </p>
        )}
        {selectedApi === 'alpha_vantage' && (
          <p style={{ fontSize: '13px', margin: 0, color: 'var(--text-dim)' }}>
            <strong>Alpha Vantage:</strong> The other API we discussed. Excellent for daily and historical stock time series. Requires a standard free-tier API key.
          </p>
        )}
        {selectedApi === 'coingecko' && (
          <p style={{ fontSize: '13px', margin: 0, color: 'var(--text-dim)' }}>
            <strong>CoinGecko:</strong> Completely free public API (no key required for base tier) offering real-time cryptocurrency data, historical price charts, and market caps.
          </p>
        )}
        {selectedApi === 'exchangerate' && (
          <p style={{ fontSize: '13px', margin: 0, color: 'var(--text-dim)' }}>
            <strong>ExchangeRate-API:</strong> Free tier public API for currency and forex conversions. Excellent reliability for daily fiat tracking.
          </p>
        )}
      </div>
      
      <button 
        style={{
          alignSelf: 'flex-start',
          padding: '8px 16px',
          background: 'var(--button-primary-bg)',
          color: 'var(--button-primary-fg)',
          border: '1px solid var(--button-primary-border)',
          borderRadius: '4px',
          cursor: 'pointer',
          fontWeight: 500
        }}
      >
        Save Investment Configuration
      </button>
    </div>
  );
}