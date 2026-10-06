use std::collections::HashMap;

use serde::Deserialize;

use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Limit {
    Unlimited,
    Value(f64),
}

impl Limit {
    fn max(self, other: Limit) -> Limit {
        match (self, other) {
            (Limit::Unlimited, _) | (_, Limit::Unlimited) => Limit::Unlimited,
            (Limit::Value(a), Limit::Value(b)) => Limit::Value(a.max(b)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum FeatureType {
    Boolean,
    Numeric,
    Enum,
}

impl FeatureType {
    fn name(self) -> &'static str {
        match self {
            FeatureType::Boolean => "BOOLEAN",
            FeatureType::Numeric => "NUMERIC",
            FeatureType::Enum => "ENUM",
        }
    }
}

#[derive(Deserialize)]
struct WireSnapshot {
    features: Vec<WireFeature>,
    product_versions: Vec<WireProductVersion>,
    customers: HashMap<String, Vec<String>>,
}

#[derive(Deserialize)]
struct WireFeature {
    key: String,
    #[serde(rename = "type")]
    ty: FeatureType,
}

#[derive(Deserialize)]
struct WireProductVersion {
    token: String,
    entitlements: Vec<WireEntitlement>,
}

#[derive(Deserialize)]
struct WireEntitlement {
    feature: String,
    value: serde_json::Value,
}

#[derive(Debug)]
enum Value {
    Bool(bool),
    Numeric(Limit),
    Enum(String),
}

/// Immutable, pre-indexed entitlement data. Checks are hash lookups only.
#[derive(Debug)]
pub struct Snapshot {
    features: HashMap<String, FeatureType>,
    product_versions: Vec<HashMap<String, Value>>,
    customers: HashMap<String, Vec<usize>>,
}

impl Snapshot {
    pub fn from_json(bytes: &[u8]) -> Result<Snapshot, Error> {
        let wire: WireSnapshot =
            serde_json::from_slice(bytes).map_err(|e| Error::InvalidSnapshot(e.to_string()))?;
        Self::from_wire(wire)
    }

    fn from_wire(wire: WireSnapshot) -> Result<Snapshot, Error> {
        let invalid = |msg: String| Err(Error::InvalidSnapshot(msg));

        let features: HashMap<String, FeatureType> =
            wire.features.into_iter().map(|f| (f.key, f.ty)).collect();

        let mut pv_index: HashMap<String, usize> = HashMap::new();
        let mut product_versions = Vec::with_capacity(wire.product_versions.len());
        for pv in wire.product_versions {
            let mut entitlements = HashMap::with_capacity(pv.entitlements.len());
            for e in pv.entitlements {
                let Some(&ty) = features.get(&e.feature) else {
                    return invalid(format!(
                        "{} references unknown feature {}",
                        pv.token, e.feature
                    ));
                };
                let value = match (ty, &e.value) {
                    (FeatureType::Boolean, serde_json::Value::Bool(b)) => Value::Bool(*b),
                    (FeatureType::Numeric, serde_json::Value::Number(n)) => {
                        Value::Numeric(Limit::Value(n.as_f64().unwrap_or(f64::NAN)))
                    }
                    (FeatureType::Numeric, serde_json::Value::String(s)) if s == "unlimited" => {
                        Value::Numeric(Limit::Unlimited)
                    }
                    (FeatureType::Enum, serde_json::Value::String(s)) => Value::Enum(s.clone()),
                    _ => {
                        return invalid(format!(
                            "{}: value for {} does not match feature type {}",
                            pv.token,
                            e.feature,
                            ty.name()
                        ));
                    }
                };
                entitlements.insert(e.feature, value);
            }
            pv_index.insert(pv.token, product_versions.len());
            product_versions.push(entitlements);
        }

        let mut customers = HashMap::with_capacity(wire.customers.len());
        for (customer, tokens) in wire.customers {
            let mut idxs = Vec::with_capacity(tokens.len());
            for t in tokens {
                match pv_index.get(&t) {
                    Some(&i) => idxs.push(i),
                    None => {
                        return invalid(format!(
                            "{customer} references unknown product version {t}"
                        ));
                    }
                }
            }
            customers.insert(customer, idxs);
        }

        Ok(Snapshot {
            features,
            product_versions,
            customers,
        })
    }

    fn values<'a>(
        &'a self,
        customer: &str,
        feature: &'a str,
        expected: FeatureType,
    ) -> Result<impl Iterator<Item = &'a Value> + 'a, Error> {
        let ty = *self
            .features
            .get(feature)
            .ok_or_else(|| Error::FeatureNotFound(feature.to_string()))?;
        if ty != expected {
            return Err(Error::WrongType {
                feature: feature.to_string(),
                expected: expected.name(),
                actual: ty.name(),
            });
        }
        let pvs = self
            .customers
            .get(customer)
            .ok_or_else(|| Error::CustomerNotFound(customer.to_string()))?;
        Ok(pvs
            .iter()
            .filter_map(move |&i| self.product_versions[i].get(feature)))
    }

    pub fn check_bool(&self, customer: &str, feature: &str) -> Result<bool, Error> {
        Ok(self
            .values(customer, feature, FeatureType::Boolean)?
            .any(|v| matches!(v, Value::Bool(true))))
    }

    pub fn check_numeric(&self, customer: &str, feature: &str) -> Result<Option<Limit>, Error> {
        Ok(self
            .values(customer, feature, FeatureType::Numeric)?
            .filter_map(|v| match v {
                Value::Numeric(l) => Some(*l),
                _ => None,
            })
            .reduce(Limit::max))
    }

    pub fn check_enum(&self, customer: &str, feature: &str) -> Result<Option<String>, Error> {
        Ok(self
            .values(customer, feature, FeatureType::Enum)?
            .find_map(|v| match v {
                Value::Enum(s) => Some(s.clone()),
                _ => None,
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{
      "features": [
        {"key": "sso", "type": "BOOLEAN"},
        {"key": "seats", "type": "NUMERIC"},
        {"key": "tier", "type": "ENUM"}
      ],
      "product_versions": [
        {"token": "pv_free", "entitlements": [
          {"feature": "sso", "value": false},
          {"feature": "seats", "value": 3},
          {"feature": "tier", "value": "community"}
        ]},
        {"token": "pv_pro", "entitlements": [
          {"feature": "sso", "value": true},
          {"feature": "seats", "value": 25},
          {"feature": "tier", "value": "priority"}
        ]},
        {"token": "pv_unl", "entitlements": [{"feature": "seats", "value": "unlimited"}]},
        {"token": "pv_empty", "entitlements": []}
      ],
      "customers": {
        "free": ["pv_free"],
        "pro": ["pv_free", "pv_pro"],
        "unl": ["pv_pro", "pv_unl"],
        "none": ["pv_empty"]
      }
    }"#;

    fn snap() -> Snapshot {
        Snapshot::from_json(JSON.as_bytes()).unwrap()
    }

    #[test]
    fn bool_is_or() {
        let s = snap();
        assert_eq!(s.check_bool("free", "sso"), Ok(false));
        assert_eq!(s.check_bool("pro", "sso"), Ok(true));
        assert_eq!(s.check_bool("none", "sso"), Ok(false));
    }

    #[test]
    fn numeric_is_max_and_unlimited_wins() {
        let s = snap();
        assert_eq!(s.check_numeric("free", "seats"), Ok(Some(Limit::Value(3.0))));
        assert_eq!(s.check_numeric("pro", "seats"), Ok(Some(Limit::Value(25.0))));
        assert_eq!(s.check_numeric("unl", "seats"), Ok(Some(Limit::Unlimited)));
        assert_eq!(s.check_numeric("none", "seats"), Ok(None));
    }

    #[test]
    fn enum_is_first_match() {
        let s = snap();
        assert_eq!(s.check_enum("pro", "tier"), Ok(Some("community".into())));
        assert_eq!(s.check_enum("unl", "tier"), Ok(Some("priority".into())));
        assert_eq!(s.check_enum("none", "tier"), Ok(None));
    }

    #[test]
    fn errors() {
        let s = snap();
        assert!(matches!(
            s.check_bool("nope", "sso"),
            Err(Error::CustomerNotFound(_))
        ));
        assert!(matches!(
            s.check_bool("free", "nope"),
            Err(Error::FeatureNotFound(_))
        ));
        assert!(matches!(
            s.check_bool("free", "seats"),
            Err(Error::WrongType { .. })
        ));
    }

    #[test]
    fn rejects_type_mismatch() {
        let bad = JSON.replace(
            r#"{"feature": "sso", "value": true}"#,
            r#"{"feature": "sso", "value": 1}"#,
        );
        assert!(matches!(
            Snapshot::from_json(bad.as_bytes()),
            Err(Error::InvalidSnapshot(_))
        ));
    }

    #[test]
    fn rejects_unknown_product_version() {
        let bad = JSON.replace(r#""free": ["pv_free"]"#, r#""free": ["pv_missing"]"#);
        assert!(matches!(
            Snapshot::from_json(bad.as_bytes()),
            Err(Error::InvalidSnapshot(_))
        ));
    }
}
