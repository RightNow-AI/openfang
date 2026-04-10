/**
 * OpenFang i18n (Internationalization) Module
 * 
 * Provides runtime language switching for the OpenFang dashboard UI.
 * Supports English (default) and Russian.
 * 
 * Usage:
 *   - HTML: <span data-i18n="nav.overview">Overview</span>
 *   - JS:   window.t('nav.overview')
 *   - Auto-applies translations on load based on stored/preferred language
 */

(function() {
  'use strict';

  // Language store
  let currentLang = 'en';
  let translations = {};
  let isInitialized = false;

  /**
   * Load translations from a JSON file
   * @param {string} lang - Language code (en, ru)
   * @returns {Promise<Object>} Translation object
   */
  async function loadTranslations(lang) {
    try {
      // Use cached translations if available
      if (window.__i18nCache && window.__i18nCache[lang]) {
        return window.__i18nCache[lang];
      }

      const response = await fetch(`/i18n/${lang}.json`);
      if (!response.ok) {
        console.warn(`[i18n] Failed to load ${lang}.json, falling back to en`);
        if (lang !== 'en') {
          return loadTranslations('en');
        }
        return {};
      }

      const data = await response.json();
      
      // Cache for future use
      if (!window.__i18nCache) window.__i18nCache = {};
      window.__i18nCache[lang] = data;
      
      return data;
    } catch (error) {
      console.error(`[i18n] Error loading translations for ${lang}:`, error);
      if (lang !== 'en') {
        return loadTranslations('en');
      }
      return {};
    }
  }

  /**
   * Get a translated string by key
   * @param {string} key - Translation key (e.g., 'nav.overview')
   * @param {Object} params - Optional interpolation parameters
   * @returns {string} Translated string or key if not found
   */
  function t(key, params) {
    if (!isInitialized) {
      console.warn('[i18n] Not initialized, returning key');
      return key;
    }

    let text = translations[key] || key;

    // Handle interpolation (e.g., 'Hello, {{name}}')
    if (params && typeof params === 'object') {
      Object.keys(params).forEach(param => {
        text = text.replace(new RegExp(`{{${param}}}`, 'g'), params[param]);
      });
    }

    return text;
  }

  /**
   * Apply translations to all elements with data-i18n attribute
   * Also updates the <html> lang attribute
   */
  function applyTranslations() {
    // Update document language
    document.documentElement.lang = currentLang;
    
    // Find and translate all elements with data-i18n attribute
    const elements = document.querySelectorAll('[data-i18n]');
    elements.forEach(el => {
      const key = el.getAttribute('data-i18n');
      const translation = t(key);
      
      // Check if element is a form input/textarea
      if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') {
        // For form elements, only update if it's a placeholder or aria-label
        if (el.hasAttribute('placeholder')) {
          el.placeholder = translation;
        }
        if (el.hasAttribute('aria-label')) {
          el.setAttribute('aria-label', translation);
        }
        if (el.hasAttribute('title')) {
          el.setAttribute('title', translation);
        }
      } else {
        // For regular elements, update text content
        el.textContent = translation;
      }
    });

    // Update elements with data-i18n-* attributes for attributes
    const attrElements = document.querySelectorAll('[data-i18n-placeholder], [data-i18n-title], [data-i18n-aria-label]');
    attrElements.forEach(el => {
      if (el.hasAttribute('data-i18n-placeholder')) {
        el.placeholder = t(el.getAttribute('data-i18n-placeholder'));
      }
      if (el.hasAttribute('data-i18n-title')) {
        el.title = t(el.getAttribute('data-i18n-title'));
      }
      if (el.hasAttribute('data-i18n-aria-label')) {
        el.setAttribute('aria-label', t(el.getAttribute('data-i18n-aria-label')));
      }
    });

    // Update meta tags
    const metaDesc = document.querySelector('meta[name="description"]');
    if (metaDesc) {
      const desc = t('app.description', { name: 'OpenFang' });
      if (desc !== 'app.description') {
        metaDesc.content = desc;
      }
    }

    console.log(`[i18n] Applied translations for language: ${currentLang}`);
  }

  /**
   * Set the current language and apply translations
   * @param {string} lang - Language code (en, ru)
   * @param {boolean} persist - Whether to save to localStorage
   */
  async function setLanguage(lang, persist = true) {
    if (!['en', 'ru'].includes(lang)) {
      console.warn(`[i18n] Unknown language: ${lang}, defaulting to en`);
      lang = 'en';
    }

    currentLang = lang;
    translations = await loadTranslations(lang);
    isInitialized = true;

    // Save preference
    if (persist) {
      localStorage.setItem('openfang_language', lang);
    }

    // Apply to DOM
    applyTranslations();

    // Dispatch event for Alpine.js components to react
    window.dispatchEvent(new CustomEvent('i18n:language-changed', { 
      detail: { language: lang } 
    }));
  }

  /**
   * Get the current language
   * @returns {string} Current language code
   */
  function getLanguage() {
    return currentLang;
  }

  /**
   * Initialize i18n system
   * Loads language preference and applies translations
   */
  async function init() {
    // Determine language priority:
    // 1. localStorage (user preference)
    // 2. Browser language
    // 3. Default to English

    let lang = localStorage.getItem('openfang_language');
    
    if (!lang) {
      // Try to detect browser language
      const browserLang = navigator.language || navigator.userLanguage || '';
      if (browserLang.startsWith('ru')) {
        lang = 'ru';
      } else {
        lang = 'en';
      }
    }

    await setLanguage(lang, false);
  }

  /**
   * Get available languages
   * @returns {Array<{code: string, name: string}>}
   */
  function getAvailableLanguages() {
    return [
      { code: 'en', name: 'English' },
      { code: 'ru', name: 'Русский' }
    ];
  }

  // Expose to global scope
  window.i18n = {
    t,
    setLanguage,
    getLanguage,
    getAvailableLanguages,
    init,
    isInitialized: () => isInitialized
  };

  // Auto-initialize when DOM is ready
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }

})();
