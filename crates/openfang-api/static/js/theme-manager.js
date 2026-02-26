/**
 * ThemeManager — system preference detection + persistence for OpenFang
 *
 * Integrates with Alpine.js app store. Call ThemeManager.init(appComponent)
 * from app() init() to wire system preference changes automatically.
 */
const ThemeManager = (() => {
  const STORAGE_KEY = 'openfang-theme';
  const VALID = ['light', 'dark', 'system'];
  const QUERY = '(prefers-color-scheme: dark)';

  function systemTheme() {
    return window.matchMedia && window.matchMedia(QUERY).matches ? 'dark' : 'light';
  }

  function resolve(theme) {
    return theme === 'system' ? systemTheme() : theme;
  }

  function applyToDOM(theme) {
    const resolved = resolve(theme);
    document.documentElement.setAttribute('data-theme', resolved);
    document.documentElement.style.colorScheme = resolved;
  }

  function load() {
    const stored = localStorage.getItem(STORAGE_KEY);
    return VALID.includes(stored) ? stored : 'system';
  }

  function save(theme) {
    localStorage.setItem(STORAGE_KEY, theme);
  }

  /**
   * Init — applies theme immediately and wires system preference listener.
   * @param {object} alpineRef - Reference to Alpine component with { theme, setTheme }
   */
  function init(alpineRef) {
    const theme = load();
    applyToDOM(theme);

    if (window.matchMedia) {
      window.matchMedia(QUERY).addEventListener('change', () => {
        if (alpineRef && alpineRef.theme === 'system') {
          applyToDOM('system');
        }
      });
    }

    return theme;
  }

  function set(theme, alpineRef) {
    if (!VALID.includes(theme)) return;
    save(theme);
    applyToDOM(theme);
    if (alpineRef) alpineRef.theme = theme;
  }

  return { init, set, load, resolve, applyToDOM };
})();
