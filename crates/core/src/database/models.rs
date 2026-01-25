//! Database model types
//!
//! Rust types representing database records.

use std::path::PathBuf;

/// Type of package installation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallType {
    Formula,
    Cask,
}

impl InstallType {
    pub fn as_str(&self) -> &'static str {
        match self {
            InstallType::Formula => "formula",
            InstallType::Cask => "cask",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "formula" => Some(InstallType::Formula),
            "cask" => Some(InstallType::Cask),
            _ => None,
        }
    }
}

/// Type of dependency relationship
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyType {
    Runtime,
    Build,
    Test,
    Optional,
}

impl DependencyType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DependencyType::Runtime => "runtime",
            DependencyType::Build => "build",
            DependencyType::Test => "test",
            DependencyType::Optional => "optional",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "runtime" => Some(DependencyType::Runtime),
            "build" => Some(DependencyType::Build),
            "test" => Some(DependencyType::Test),
            "optional" => Some(DependencyType::Optional),
            _ => None,
        }
    }
}

/// Type of file in a package
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Bin,
    Lib,
    Include,
    Share,
    Etc,
    Other,
}

impl FileType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileType::Bin => "bin",
            FileType::Lib => "lib",
            FileType::Include => "include",
            FileType::Share => "share",
            FileType::Etc => "etc",
            FileType::Other => "other",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "bin" => Some(FileType::Bin),
            "lib" => Some(FileType::Lib),
            "include" => Some(FileType::Include),
            "share" => Some(FileType::Share),
            "etc" => Some(FileType::Etc),
            "other" => Some(FileType::Other),
            _ => None,
        }
    }
}

/// Type of installation action for history
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallAction {
    Install,
    Upgrade,
    Reinstall,
    Uninstall,
    Link,
    Unlink,
}

impl InstallAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            InstallAction::Install => "install",
            InstallAction::Upgrade => "upgrade",
            InstallAction::Reinstall => "reinstall",
            InstallAction::Uninstall => "uninstall",
            InstallAction::Link => "link",
            InstallAction::Unlink => "unlink",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "install" => Some(InstallAction::Install),
            "upgrade" => Some(InstallAction::Upgrade),
            "reinstall" => Some(InstallAction::Reinstall),
            "uninstall" => Some(InstallAction::Uninstall),
            "link" => Some(InstallAction::Link),
            "unlink" => Some(InstallAction::Unlink),
            _ => None,
        }
    }
}

/// An installed package record
#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub id: Option<i64>,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub tap: Option<String>,
    pub installed_at: i64,
    pub updated_at: Option<i64>,
    pub install_type: InstallType,
    pub build_from_source: bool,
    pub cellar_path: PathBuf,
    pub linked: bool,
    pub pinned: bool,
    pub source_sha256: Option<String>,
}

impl InstalledPackage {
    /// Create a new package record for installation
    pub fn new(name: String, version: String, cellar_path: PathBuf) -> Self {
        Self {
            id: None,
            name,
            version,
            description: None,
            homepage: None,
            license: None,
            tap: None,
            installed_at: chrono::Utc::now().timestamp(),
            updated_at: None,
            install_type: InstallType::Formula,
            build_from_source: false,
            cellar_path,
            linked: false,
            pinned: false,
            source_sha256: None,
        }
    }
}

/// A file belonging to an installed package
#[derive(Debug, Clone)]
pub struct PackageFile {
    pub id: Option<i64>,
    pub package_id: i64,
    pub file_path: PathBuf,
    pub file_type: FileType,
    pub symlink_path: Option<PathBuf>,
}

/// A dependency relationship
#[derive(Debug, Clone)]
pub struct PackageDependency {
    pub id: Option<i64>,
    pub package_id: i64,
    pub dependency_name: String,
    pub dependency_type: DependencyType,
    pub version_constraint: Option<String>,
    pub is_satisfied: bool,
}

/// A history entry for package operations
#[derive(Debug, Clone)]
pub struct InstallHistoryEntry {
    pub id: Option<i64>,
    pub package_name: String,
    pub version: String,
    pub action: InstallAction,
    pub performed_at: i64,
    pub success: bool,
    pub error_message: Option<String>,
}

impl InstallHistoryEntry {
    /// Create a new history entry
    pub fn new(package_name: String, version: String, action: InstallAction, success: bool) -> Self {
        Self {
            id: None,
            package_name,
            version,
            action,
            performed_at: chrono::Utc::now().timestamp(),
            success,
            error_message: None,
        }
    }
}

/// Summary information for package listing
#[derive(Debug, Clone)]
pub struct PackageSummary {
    pub name: String,
    pub version: String,
    pub tap: Option<String>,
    pub linked: bool,
    pub pinned: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_type_conversion() {
        assert_eq!(InstallType::Formula.as_str(), "formula");
        assert_eq!(InstallType::from_str("formula"), Some(InstallType::Formula));
        assert_eq!(InstallType::from_str("invalid"), None);
    }

    #[test]
    fn test_dependency_type_conversion() {
        assert_eq!(DependencyType::Runtime.as_str(), "runtime");
        assert_eq!(DependencyType::from_str("build"), Some(DependencyType::Build));
    }

    #[test]
    fn test_installed_package_new() {
        let pkg = InstalledPackage::new(
            "curl".to_string(),
            "8.5.0".to_string(),
            PathBuf::from("/opt/brew-rs/Cellar/curl/8.5.0"),
        );

        assert_eq!(pkg.name, "curl");
        assert_eq!(pkg.version, "8.5.0");
        assert!(pkg.id.is_none());
        assert!(!pkg.linked);
        assert!(!pkg.pinned);
    }
}
