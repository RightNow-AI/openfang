'use client';

import { useTheme } from './ThemeProvider';

const OPTIONS = [
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
  { value: 'system', label: 'System' },
];

export default function ThemeToggle() {
  const { theme, setTheme, ready } = useTheme();

  return (
    <div className="flex items-center gap-1 rounded-xl border border-[color:var(--border)] bg-[color:var(--card)] p-1">
      {OPTIONS.map((option) => (
        <button
          key={option.value}
          type="button"
          disabled={!ready}
          onClick={() => setTheme(option.value)}
          className={[
            'min-h-8 rounded-lg px-3 text-xs font-medium transition-colors',
            theme === option.value
              ? 'bg-[color:var(--accent)] text-white'
              : 'text-[color:var(--muted-foreground)] hover:bg-[color:var(--muted)]',
          ].join(' ')}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
