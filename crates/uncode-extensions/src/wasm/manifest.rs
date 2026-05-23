//! Extension manifest — companion JSON metadata for `.wasm` files.

use std::path::Path;

use super::{DEFAULT_FUEL_LIMIT, DEFAULT_MEMORY_LIMIT_MB, DEFAULT_TIMEOUT, WasmError};

/// Extension permissions (reserved for future use).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExtensionPermissions {
    #[serde(default)]
    pub filesystem: bool,
    #[serde(default)]
    pub network: bool,
}

impl Default for ExtensionPermissions {
    fn default() -> Self {
        Self {
            filesystem: false,
            network: false,
        }
    }
}

/// Manifest describing a WASM extension's metadata and sandbox config.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExtensionManifest {
    /// Extension name. Defaults to the `.wasm` filename (without extension).
    #[serde(default)]
    pub name: String,

    /// Semantic version. Defaults to "0.1.0".
    #[serde(default = "default_version")]
    pub version: String,

    /// Human-readable description.
    pub description: Option<String>,

    /// Lifecycle hooks the extension subscribes to.
    #[serde(default)]
    pub hooks: Vec<String>,

    /// Sandbox permissions.
    #[serde(default)]
    pub permissions: ExtensionPermissions,

    /// Memory limit in MB. Defaults to 64.
    pub memory_limit_mb: Option<u32>,

    /// Fuel limit (instruction count) per call. Defaults to 10M.
    pub fuel_limit: Option<u64>,

    /// Timeout per WASM call in seconds. Defaults to 5.
    pub timeout_secs: Option<u64>,
}

fn default_version() -> String {
    "0.1.0".into()
}

impl ExtensionManifest {
    /// Load manifest from a companion `.json` file next to the `.wasm` file.
    ///
    /// If the JSON file does not exist, returns a default manifest using the
    /// WASM filename as the extension name.
    pub fn load(wasm_path: &Path) -> Result<Self, WasmError> {
        let json_path = wasm_path.with_extension("json");

        if json_path.exists() {
            let content = std::fs::read_to_string(&json_path).map_err(|e| {
                WasmError::Manifest(format!("cannot read {}: {e}", json_path.display()))
            })?;
            let mut manifest: Self =
                serde_json::from_str(&content).map_err(|e| WasmError::Manifest(e.to_string()))?;

            // If name is empty, derive from filename.
            if manifest.name.is_empty() {
                manifest.name = derive_name(wasm_path);
            }

            Ok(manifest)
        } else {
            Ok(Self::default_for(wasm_path))
        }
    }

    /// Default manifest using filename-derived name.
    pub fn default_for(wasm_path: &Path) -> Self {
        Self {
            name: derive_name(wasm_path),
            version: default_version(),
            description: None,
            hooks: Vec::new(),
            permissions: ExtensionPermissions::default(),
            memory_limit_mb: None,
            fuel_limit: None,
            timeout_secs: None,
        }
    }

    /// Resolved memory limit in MB.
    pub fn memory_limit_mb(&self) -> u32 {
        self.memory_limit_mb.unwrap_or(DEFAULT_MEMORY_LIMIT_MB)
    }

    /// Resolved fuel limit.
    pub fn fuel_limit(&self) -> u64 {
        self.fuel_limit.unwrap_or(DEFAULT_FUEL_LIMIT)
    }

    /// Resolved timeout duration.
    pub fn timeout(&self) -> std::time::Duration {
        self.timeout_secs
            .map(std::time::Duration::from_secs)
            .unwrap_or(DEFAULT_TIMEOUT)
    }
}

/// Derive extension name from the `.wasm` filename.
fn derive_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}
