//! Internationalization (i18n) module for OpenFang CLI.
//!
//! This module provides localization support using Mozilla's Fluent library.
//! Language files are embedded at compile time and selected at runtime based
//! on the user's configuration.

use fluent::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use std::cell::RefCell;
use unic_langid::LanguageIdentifier;

/// Embedded language files
const EN_FTL: &str = include_str!("../locales/en/main.ftl");
const ZH_CN_FTL: &str = include_str!("../locales/zh-CN/main.ftl");

/// Supported languages
pub const SUPPORTED_LANGUAGES: &[&str] = &["en", "zh-CN"];

/// Default language
pub const DEFAULT_LANGUAGE: &str = "en";

// Thread-local storage for i18n
thread_local! {
    static I18N: RefCell<Option<I18n>> = const { RefCell::new(None) };
}

/// Internationalization handler
pub struct I18n {
    bundle: FluentBundle<FluentResource>,
    current_language: String,
}

impl I18n {
    /// Create a new i18n instance with the specified language
    pub fn new(language: &str) -> Result<Self, String> {
        let lang = if SUPPORTED_LANGUAGES.contains(&language) {
            language
        } else {
            DEFAULT_LANGUAGE
        };

        let bundle = Self::load_bundle(lang)?;
        Ok(Self {
            bundle,
            current_language: lang.to_string(),
        })
    }

    /// Load a Fluent bundle for the specified language
    fn load_bundle(language: &str) -> Result<FluentBundle<FluentResource>, String> {
        let lang_id: LanguageIdentifier = language
            .parse()
            .map_err(|e| format!("Invalid language identifier: {e}"))?;

        let mut bundle = FluentBundle::new(vec![lang_id]);

        // Get the FTL content for the requested language
        let ftl_content = match language {
            "en" => EN_FTL,
            "zh-CN" => ZH_CN_FTL,
            _ => EN_FTL, // Fallback to English
        };

        let resource = FluentResource::try_new(ftl_content.to_string())
            .map_err(|(_, errors)| format!("Failed to parse Fluent resource: {errors:?}"))?;

        bundle
            .add_resource(resource)
            .map_err(|errors| format!("Failed to add Fluent resource: {errors:?}"))?;

        Ok(bundle)
    }

    /// Get a localized message by key
    pub fn get(&self, key: &str) -> String {
        self.get_with_args(key, None)
    }

    /// Get a localized message by key with arguments
    pub fn get_with_args(&self, key: &str, args: Option<&FluentArgs>) -> String {
        let msg = match self.bundle.get_message(key) {
            Some(m) => m,
            None => return format!("[{key}]"), // Fallback to key if not found
        };

        let pattern = match msg.value() {
            Some(p) => p,
            None => return format!("[{key}]"),
        };

        let mut errors = vec![];
        let result = self.bundle.format_pattern(pattern, args, &mut errors);

        if !errors.is_empty() {
            tracing::warn!(key = %key, errors = ?errors, "Fluent formatting errors");
        }

        result.to_string()
    }

    /// Get the current language
    pub fn language(&self) -> &str {
        &self.current_language
    }
}

/// Initialize the i18n system
pub fn init(language: &str) {
    let i18n = I18n::new(language).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "Failed to initialize i18n, using default");
        I18n::new(DEFAULT_LANGUAGE).expect("Default language must be available")
    });

    I18N.with(|cell| {
        *cell.borrow_mut() = Some(i18n);
    });
}

/// Get a localized message by key
pub fn t(key: &str) -> String {
    I18N.with(|cell| {
        if let Some(ref i18n) = *cell.borrow() {
            i18n.get(key)
        } else {
            format!("[{key}]")
        }
    })
}

/// Get a localized message by key with arguments
pub fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    I18N.with(|cell| {
        if let Some(ref i18n) = *cell.borrow() {
            let mut fluent_args = FluentArgs::new();
            for (k, v) in args {
                fluent_args.set(*k, FluentValue::from(*v));
            }
            i18n.get_with_args(key, Some(&fluent_args))
        } else {
            format!("[{key}]")
        }
    })
}

/// Get the current language
pub fn current_language() -> String {
    I18N.with(|cell| {
        if let Some(ref i18n) = *cell.borrow() {
            i18n.language().to_string()
        } else {
            DEFAULT_LANGUAGE.to_string()
        }
    })
}

/// Macro for easy localization
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::t($key)
    };
    ($key:expr, $($arg_key:expr => $arg_val:expr),+ $(,)?) => {
        $crate::i18n::t_args($key, &[$(($arg_key, $arg_val)),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_language() {
        init(DEFAULT_LANGUAGE);
        assert_eq!(current_language(), "en");
    }

    #[test]
    fn test_get_message() {
        init("en");
        let msg = t("app-name");
        assert!(msg.contains("OpenFang"));
    }

    #[test]
    fn test_fallback_for_missing_key() {
        init("en");
        let msg = t("nonexistent-key");
        assert_eq!(msg, "[nonexistent-key]");
    }
}
