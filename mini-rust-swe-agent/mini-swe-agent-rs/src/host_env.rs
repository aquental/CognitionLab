//! `HostEnv`: one snapshot of the process environment (plan §2.2).
//!
//! The binary reads the environment once (`std::env::vars_os`) and passes the snapshot down;
//! nothing calls `set_var`, which is `unsafe` in edition 2024. Children get every entry as an
//! `OsString`. Templates and configuration see only entries whose name and value are UTF-8
//! (Python decodes the others with `surrogateescape`; a documented divergence, plan §2.4).

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};

use serde_json::{Map, Value};

/// A snapshot of environment variables.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostEnv {
    vars: BTreeMap<OsString, OsString>,
}

impl HostEnv {
    /// Builds a snapshot from name/value pairs, such as `std::env::vars_os()`.
    pub fn from_vars<I: IntoIterator<Item = (OsString, OsString)>>(vars: I) -> Self {
        Self {
            vars: vars.into_iter().collect(),
        }
    }

    /// The value of `name`, if it is set and is UTF-8.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.vars
            .get(OsStr::new(name))
            .and_then(|value| value.to_str())
    }

    /// Overlays a `.env` file as `load_dotenv` does: the whole file is read first (the last
    /// assignment of a name wins), then the names already set are skipped. A missing file is
    /// not an error.
    pub fn overlay_dotenv(&mut self, path: &std::path::Path) -> Result<(), dotenvy::Error> {
        if !path.is_file() {
            return Ok(());
        }
        let pairs: std::collections::BTreeMap<String, String> =
            dotenvy::from_path_iter(path).and_then(|iter| iter.collect())?;
        self.overlay_missing(pairs);
        Ok(())
    }

    /// Adds the pairs whose names are not set yet, as `load_dotenv` does without `override`.
    pub fn overlay_missing<I: IntoIterator<Item = (String, String)>>(&mut self, pairs: I) {
        for (name, value) in pairs {
            self.vars
                .entry(OsString::from(name))
                .or_insert_with(|| OsString::from(value));
        }
    }

    /// Every entry, for a child process environment.
    pub fn child_vars(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.vars
            .iter()
            .map(|(name, value)| (name.as_os_str(), value.as_os_str()))
    }

    /// The UTF-8 entries as a JSON object, for template variables.
    pub fn template_vars(&self) -> Map<String, Value> {
        self.vars
            .iter()
            .filter_map(|(name, value)| {
                Some((
                    name.to_str()?.to_owned(),
                    Value::String(value.to_str()?.to_owned()),
                ))
            })
            .collect()
    }
}
