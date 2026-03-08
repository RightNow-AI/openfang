//! Lenient serde deserializers for backwards-compatible agent manifest loading.
//!
//! When agent manifests are stored as msgpack blobs in SQLite, schema changes
//! (e.g., a field changing from integer to struct, or from map to Vec) cause
//! hard deserialization failures. These helpers gracefully return defaults
//! for type-mismatched fields instead of failing the entire deserialization.

use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;

/// Deserialize a `Vec<T>` leniently: if the stored value is not a sequence
/// (e.g., it's a map, integer, string, bool, or null), return an empty Vec
/// instead of failing.
pub fn vec_lenient<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct VecLenientVisitor<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for VecLenientVisitor<T> {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a sequence (or any value, which will default to empty Vec)")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut vec = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(item) = seq.next_element()? {
                vec.push(item);
            }
            Ok(vec)
        }

        // All non-sequence types return empty Vec
        fn visit_map<A>(self, mut _map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            // Drain the map to keep the deserializer state consistent
            while let Some((_, _)) = _map.next_entry::<de::IgnoredAny, de::IgnoredAny>()? {}
            Ok(Vec::new())
        }

        fn visit_i64<E: de::Error>(self, _v: i64) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_u64<E: de::Error>(self, _v: u64) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_f64<E: de::Error>(self, _v: f64) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_str<E: de::Error>(self, _v: &str) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_bool<E: de::Error>(self, _v: bool) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }
    }

    deserializer.deserialize_any(VecLenientVisitor(PhantomData))
}

/// Deserialize a `HashMap<K, V>` leniently: if the stored value is not a map
/// (e.g., it's a sequence, integer, string, bool, or null), return an empty
/// HashMap instead of failing.
pub fn map_lenient<'de, D, K, V>(deserializer: D) -> Result<HashMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Eq + Hash,
    V: Deserialize<'de>,
{
    struct MapLenientVisitor<K, V>(PhantomData<(K, V)>);

    impl<'de, K, V> Visitor<'de> for MapLenientVisitor<K, V>
    where
        K: Deserialize<'de> + Eq + Hash,
        V: Deserialize<'de>,
    {
        type Value = HashMap<K, V>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map (or any value, which will default to empty HashMap)")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut result = HashMap::with_capacity(map.size_hint().unwrap_or(0));
            while let Some((k, v)) = map.next_entry()? {
                result.insert(k, v);
            }
            Ok(result)
        }

        // All non-map types return empty HashMap
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            // Drain the sequence to keep the deserializer state consistent
            while seq.next_element::<de::IgnoredAny>()?.is_some() {}
            Ok(HashMap::new())
        }

        fn visit_i64<E: de::Error>(self, _v: i64) -> Result<Self::Value, E> {
            Ok(HashMap::new())
        }

        fn visit_u64<E: de::Error>(self, _v: u64) -> Result<Self::Value, E> {
            Ok(HashMap::new())
        }

        fn visit_f64<E: de::Error>(self, _v: f64) -> Result<Self::Value, E> {
            Ok(HashMap::new())
        }

        fn visit_str<E: de::Error>(self, _v: &str) -> Result<Self::Value, E> {
            Ok(HashMap::new())
        }

        fn visit_bool<E: de::Error>(self, _v: bool) -> Result<Self::Value, E> {
            Ok(HashMap::new())
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(HashMap::new())
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(HashMap::new())
        }
    }

    deserializer.deserialize_any(MapLenientVisitor(PhantomData))
}

/// Deserialize an `Option<ExecPolicy>` leniently: accepts either a string
/// shorthand (e.g., `"allow"`, `"deny"`, `"full"`, `"allowlist"`) which maps
/// to `ExecPolicy { mode: <parsed>, ..Default::default() }`, or the full
/// struct/table form. Returns `None` for null/missing.
pub fn exec_policy_lenient<'de, D>(
    deserializer: D,
) -> Result<Option<crate::config::ExecPolicy>, D::Error>
where
    D: Deserializer<'de>,
{
    struct ExecPolicyVisitor;

    impl<'de> Visitor<'de> for ExecPolicyVisitor {
        type Value = Option<crate::config::ExecPolicy>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str(
                "a string shorthand (\"allow\", \"deny\", \"full\", \"allowlist\") or an ExecPolicy table",
            )
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            let mode = match v.to_lowercase().as_str() {
                "deny" | "none" | "disabled" => crate::config::ExecSecurityMode::Deny,
                "allowlist" | "restricted" => crate::config::ExecSecurityMode::Allowlist,
                "full" | "allow" | "all" | "unrestricted" => crate::config::ExecSecurityMode::Full,
                other => {
                    return Err(de::Error::unknown_variant(
                        other,
                        &[
                            "deny", "none", "disabled", "allowlist", "restricted", "full",
                            "allow", "all", "unrestricted",
                        ],
                    ));
                }
            };
            Ok(Some(crate::config::ExecPolicy {
                mode,
                ..Default::default()
            }))
        }

        fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let policy = crate::config::ExecPolicy::deserialize(
                de::value::MapAccessDeserializer::new(map),
            )?;
            Ok(Some(policy))
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    deserializer.deserialize_any(ExecPolicyVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestVec {
        #[serde(default, deserialize_with = "vec_lenient")]
        items: Vec<String>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestMap {
        #[serde(default, deserialize_with = "map_lenient")]
        items: HashMap<String, i32>,
    }

    // --- vec_lenient tests ---

    #[test]
    fn vec_lenient_accepts_sequence() {
        let json = r#"{"items": ["a", "b", "c"]}"#;
        let result: TestVec = serde_json::from_str(json).unwrap();
        assert_eq!(result.items, vec!["a", "b", "c"]);
    }

    #[test]
    fn vec_lenient_given_map_returns_empty() {
        let json = r#"{"items": {"key": "value"}}"#;
        let result: TestVec = serde_json::from_str(json).unwrap();
        assert!(result.items.is_empty());
    }

    #[test]
    fn vec_lenient_given_integer_returns_empty() {
        let json = r#"{"items": 268435456}"#;
        let result: TestVec = serde_json::from_str(json).unwrap();
        assert!(result.items.is_empty());
    }

    #[test]
    fn vec_lenient_given_string_returns_empty() {
        let json = r#"{"items": "not a vec"}"#;
        let result: TestVec = serde_json::from_str(json).unwrap();
        assert!(result.items.is_empty());
    }

    #[test]
    fn vec_lenient_given_bool_returns_empty() {
        let json = r#"{"items": true}"#;
        let result: TestVec = serde_json::from_str(json).unwrap();
        assert!(result.items.is_empty());
    }

    #[test]
    fn vec_lenient_given_null_returns_empty() {
        let json = r#"{"items": null}"#;
        let result: TestVec = serde_json::from_str(json).unwrap();
        assert!(result.items.is_empty());
    }

    // --- map_lenient tests ---

    #[test]
    fn map_lenient_accepts_map() {
        let json = r#"{"items": {"a": 1, "b": 2}}"#;
        let result: TestMap = serde_json::from_str(json).unwrap();
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items["a"], 1);
        assert_eq!(result.items["b"], 2);
    }

    #[test]
    fn map_lenient_given_sequence_returns_empty() {
        let json = r#"{"items": [1, 2, 3]}"#;
        let result: TestMap = serde_json::from_str(json).unwrap();
        assert!(result.items.is_empty());
    }

    #[test]
    fn map_lenient_given_integer_returns_empty() {
        let json = r#"{"items": 42}"#;
        let result: TestMap = serde_json::from_str(json).unwrap();
        assert!(result.items.is_empty());
    }

    #[test]
    fn map_lenient_given_string_returns_empty() {
        let json = r#"{"items": "not a map"}"#;
        let result: TestMap = serde_json::from_str(json).unwrap();
        assert!(result.items.is_empty());
    }

    #[test]
    fn map_lenient_given_bool_returns_empty() {
        let json = r#"{"items": false}"#;
        let result: TestMap = serde_json::from_str(json).unwrap();
        assert!(result.items.is_empty());
    }

    #[test]
    fn map_lenient_given_null_returns_empty() {
        let json = r#"{"items": null}"#;
        let result: TestMap = serde_json::from_str(json).unwrap();
        assert!(result.items.is_empty());
    }

    // --- msgpack round-trip test (simulates the actual agent manifest scenario) ---

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct OldManifest {
        name: String,
        fallback_models: u64,            // old format: integer
        skills: HashMap<String, String>, // old format: map
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct NewManifest {
        name: String,
        #[serde(default, deserialize_with = "vec_lenient")]
        fallback_models: Vec<String>, // new format: Vec
        #[serde(default, deserialize_with = "vec_lenient")]
        skills: Vec<String>, // new format: Vec
    }

    #[test]
    fn msgpack_old_format_deserializes_leniently() {
        // Serialize with the OLD schema
        let old = OldManifest {
            name: "test-agent".to_string(),
            fallback_models: 268435456,
            skills: {
                let mut m = HashMap::new();
                m.insert("web-search".to_string(), "enabled".to_string());
                m
            },
        };
        let blob = rmp_serde::to_vec_named(&old).unwrap();

        // Deserialize with the NEW schema — should succeed with empty defaults
        let new: NewManifest = rmp_serde::from_slice(&blob).unwrap();
        assert_eq!(new.name, "test-agent");
        assert!(new.fallback_models.is_empty());
        assert!(new.skills.is_empty());
    }

    // --- exec_policy_lenient tests ---

    #[derive(Debug, Deserialize)]
    struct TestExecPolicy {
        #[serde(default, deserialize_with = "exec_policy_lenient")]
        exec_policy: Option<crate::config::ExecPolicy>,
    }

    #[test]
    fn exec_policy_string_allow() {
        let toml_str = r#"exec_policy = "allow""#;
        let parsed: TestExecPolicy = toml::from_str(toml_str).unwrap();
        let policy = parsed.exec_policy.unwrap();
        assert_eq!(policy.mode, crate::config::ExecSecurityMode::Full);
        // Should have default safe_bins, timeout, etc.
        assert!(!policy.safe_bins.is_empty());
        assert_eq!(policy.timeout_secs, 30);
    }

    #[test]
    fn exec_policy_string_deny() {
        let toml_str = r#"exec_policy = "deny""#;
        let parsed: TestExecPolicy = toml::from_str(toml_str).unwrap();
        let policy = parsed.exec_policy.unwrap();
        assert_eq!(policy.mode, crate::config::ExecSecurityMode::Deny);
    }

    #[test]
    fn exec_policy_string_full() {
        let toml_str = r#"exec_policy = "full""#;
        let parsed: TestExecPolicy = toml::from_str(toml_str).unwrap();
        let policy = parsed.exec_policy.unwrap();
        assert_eq!(policy.mode, crate::config::ExecSecurityMode::Full);
    }

    #[test]
    fn exec_policy_string_allowlist() {
        let toml_str = r#"exec_policy = "allowlist""#;
        let parsed: TestExecPolicy = toml::from_str(toml_str).unwrap();
        let policy = parsed.exec_policy.unwrap();
        assert_eq!(policy.mode, crate::config::ExecSecurityMode::Allowlist);
    }

    #[test]
    fn exec_policy_table_form() {
        let toml_str = r#"
[exec_policy]
mode = "full"
timeout_secs = 60
"#;
        let parsed: TestExecPolicy = toml::from_str(toml_str).unwrap();
        let policy = parsed.exec_policy.unwrap();
        assert_eq!(policy.mode, crate::config::ExecSecurityMode::Full);
        assert_eq!(policy.timeout_secs, 60);
    }

    #[test]
    fn exec_policy_missing_is_none() {
        let toml_str = r#"other_field = true"#;
        // Use a struct with an extra ignored field
        #[derive(Debug, Deserialize)]
        struct Wrapper {
            #[serde(default, deserialize_with = "exec_policy_lenient")]
            exec_policy: Option<crate::config::ExecPolicy>,
            #[allow(dead_code)]
            #[serde(default)]
            other_field: bool,
        }
        let parsed: Wrapper = toml::from_str(toml_str).unwrap();
        assert!(parsed.exec_policy.is_none());
    }

    #[test]
    fn exec_policy_string_invalid_errors() {
        let toml_str = r#"exec_policy = "banana""#;
        let result = toml::from_str::<TestExecPolicy>(toml_str);
        assert!(result.is_err());
    }
}
