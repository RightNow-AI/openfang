'use client';

import { createContext, useContext, useEffect, useMemo, useState } from 'react';

const ThemeContext = createContext(null);
const STORAGE_KEY = 'openfang-theme';

function applyTheme(theme) {
  const root = document.documentElement;
  const resolved =
    theme === 'system'
      ? (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')
      : theme;

  root.classList.toggle('dark', resolved === 'dark');
  root.setAttribute('data-theme', resolved);
}

export function ThemeProvider({ children }) {
  const [theme, setTheme] = useState('system');
  const [ready, setReady] = useState(false);

  useEffect(() => {
    const saved = window.localStorage.getItem(STORAGE_KEY) || 'system';
    setTheme(saved);
    applyTheme(saved);
    setReady(true);

    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = () => {
      const current = window.localStorage.getItem(STORAGE_KEY) || 'system';
      if (current === 'system') applyTheme('system');
    };

    media.addEventListener?.('change', onChange);
    return () => media.removeEventListener?.('change', onChange);
  }, []);

  const value = useMemo(() => ({
    theme,
    ready,
    setTheme: (next) => {
      setTheme(next);
      window.localStorage.setItem(STORAGE_KEY, next);
      applyTheme(next);
    },
  }), [theme, ready]);

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error('useTheme must be used inside ThemeProvider');
  }
  return ctx;
}
