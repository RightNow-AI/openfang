/**
 * OpenFang Web UI - Internationalization (i18n) Module
 * 
 * A lightweight i18n system for the OpenFang dashboard.
 * Loads translations from JSON files and provides simple API for localization.
 */

(function(global) {
    'use strict';

    // Supported languages
    const SUPPORTED_LANGUAGES = ['en', 'zh-CN'];
    const DEFAULT_LANGUAGE = 'en';

    // Current language and translations cache
    let currentLanguage = DEFAULT_LANGUAGE;
    let translations = {};
    let isLoaded = false;

    /**
     * Detect user's preferred language from browser settings
     */
    function detectLanguage() {
        // Check localStorage first
        const stored = localStorage.getItem('openfang-language');
        if (stored && SUPPORTED_LANGUAGES.includes(stored)) {
            return stored;
        }

        // Check browser language
        const browserLang = navigator.language || navigator.userLanguage;
        
        // Exact match
        if (SUPPORTED_LANGUAGES.includes(browserLang)) {
            return browserLang;
        }
        
        // Check language prefix (e.g., 'zh' for 'zh-CN', 'zh-TW')
        const langPrefix = browserLang.split('-')[0];
        const match = SUPPORTED_LANGUAGES.find(lang => lang.startsWith(langPrefix));
        if (match) {
            return match;
        }

        return DEFAULT_LANGUAGE;
    }

    /**
     * Load translations for a specific language
     */
    async function loadTranslations(lang) {
        try {
            const response = await fetch(`/locales/${lang}.json`);
            if (!response.ok) {
                console.warn(`Failed to load translations for ${lang}, falling back to ${DEFAULT_LANGUAGE}`);
                if (lang !== DEFAULT_LANGUAGE) {
                    return loadTranslations(DEFAULT_LANGUAGE);
                }
                return {};
            }
            return await response.json();
        } catch (error) {
            console.error(`Error loading translations for ${lang}:`, error);
            if (lang !== DEFAULT_LANGUAGE) {
                return loadTranslations(DEFAULT_LANGUAGE);
            }
            return {};
        }
    }

    /**
     * Get nested value from object using dot notation
     */
    function getNestedValue(obj, path) {
        return path.split('.').reduce((current, key) => {
            return current && current[key] !== undefined ? current[key] : null;
        }, obj);
    }

    /**
     * Replace placeholders in a string with values
     * Supports {key} syntax
     */
    function replacePlaceholders(str, params) {
        if (!params || typeof str !== 'string') return str;
        return str.replace(/\{(\w+)\}/g, (match, key) => {
            return params[key] !== undefined ? params[key] : match;
        });
    }

    /**
     * Translate a key to the current language
     * @param {string} key - Translation key (e.g., 'nav.chat')
     * @param {object} params - Optional parameters for placeholders
     * @returns {string} Translated string or key if not found
     */
    function t(key, params) {
        const value = getNestedValue(translations, key);
        if (value === null) {
            console.warn(`Missing translation for key: ${key}`);
            return `[${key}]`;
        }
        return replacePlaceholders(value, params);
    }

    /**
     * Initialize i18n with optional language override
     */
    async function init(lang) {
        currentLanguage = lang || detectLanguage();
        translations = await loadTranslations(currentLanguage);
        isLoaded = true;
        
        // Store the language preference
        localStorage.setItem('openfang-language', currentLanguage);
        
        // Update DOM elements with data-i18n attribute
        updateDOM();
        
        // Dispatch event for components that need to react
        window.dispatchEvent(new CustomEvent('i18n-loaded', { 
            detail: { language: currentLanguage } 
        }));
        
        return currentLanguage;
    }

    /**
     * Change the current language
     */
    async function setLanguage(lang) {
        if (!SUPPORTED_LANGUAGES.includes(lang)) {
            console.warn(`Unsupported language: ${lang}`);
            return false;
        }
        
        currentLanguage = lang;
        translations = await loadTranslations(lang);
        localStorage.setItem('openfang-language', lang);
        
        updateDOM();
        
        window.dispatchEvent(new CustomEvent('i18n-changed', { 
            detail: { language: lang } 
        }));
        
        return true;
    }

    /**
     * Get the current language
     */
    function getLanguage() {
        return currentLanguage;
    }

    /**
     * Get list of supported languages
     */
    function getSupportedLanguages() {
        return [...SUPPORTED_LANGUAGES];
    }

    /**
     * Update all DOM elements with data-i18n attribute
     */
    function updateDOM() {
        document.querySelectorAll('[data-i18n]').forEach(element => {
            const key = element.getAttribute('data-i18n');
            const translated = t(key);
            if (!translated.startsWith('[')) {
                element.textContent = translated;
            }
        });

        // Update elements with data-i18n-placeholder attribute
        document.querySelectorAll('[data-i18n-placeholder]').forEach(element => {
            const key = element.getAttribute('data-i18n-placeholder');
            const translated = t(key);
            if (!translated.startsWith('[')) {
                element.placeholder = translated;
            }
        });

        // Update elements with data-i18n-title attribute
        document.querySelectorAll('[data-i18n-title]').forEach(element => {
            const key = element.getAttribute('data-i18n-title');
            const translated = t(key);
            if (!translated.startsWith('[')) {
                element.title = translated;
            }
        });
    }

    /**
     * Check if translations are loaded
     */
    function isReady() {
        return isLoaded;
    }

    // Export the i18n API
    const i18n = {
        init,
        t,
        setLanguage,
        getLanguage,
        getSupportedLanguages,
        updateDOM,
        isReady,
        DEFAULT_LANGUAGE,
        SUPPORTED_LANGUAGES
    };

    // Make it globally available
    global.i18n = i18n;

    // Also support ES module-like access
    if (typeof module !== 'undefined' && module.exports) {
        module.exports = i18n;
    }

})(typeof window !== 'undefined' ? window : this);
