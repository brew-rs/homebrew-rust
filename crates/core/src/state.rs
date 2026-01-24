//! Package state management

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageState {
    pub name: String,
    pub version: String,
    pub installed_at: i64,
    pub dependencies: Vec<String>,
}

pub struct StateManager {
    packages: HashMap<String, PackageState>,
}

impl StateManager {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
        }
    }

    pub fn add_package(&mut self, state: PackageState) {
        self.packages.insert(state.name.clone(), state);
    }

    pub fn get_package(&self, name: &str) -> Option<&PackageState> {
        self.packages.get(name)
    }
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}
