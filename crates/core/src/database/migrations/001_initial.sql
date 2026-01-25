-- Initial schema for brew-rs package database
-- Version: 001

-- Installed packages
CREATE TABLE IF NOT EXISTS packages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    version TEXT NOT NULL,
    description TEXT,
    homepage TEXT,
    license TEXT,
    tap TEXT,
    installed_at INTEGER NOT NULL,
    updated_at INTEGER,
    install_type TEXT DEFAULT 'formula' CHECK (install_type IN ('formula', 'cask')),
    build_from_source INTEGER DEFAULT 0,
    cellar_path TEXT NOT NULL,
    linked INTEGER DEFAULT 0,
    pinned INTEGER DEFAULT 0,
    source_sha256 TEXT
);

-- Package files (for uninstall tracking)
CREATE TABLE IF NOT EXISTS package_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    package_id INTEGER NOT NULL,
    file_path TEXT NOT NULL,
    file_type TEXT NOT NULL CHECK (file_type IN ('bin', 'lib', 'include', 'share', 'etc', 'other')),
    symlink_path TEXT,
    FOREIGN KEY (package_id) REFERENCES packages(id) ON DELETE CASCADE
);

-- Package dependencies (tracking what each package depends on)
CREATE TABLE IF NOT EXISTS package_dependencies (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    package_id INTEGER NOT NULL,
    dependency_name TEXT NOT NULL,
    dependency_type TEXT NOT NULL CHECK (dependency_type IN ('runtime', 'build', 'test', 'optional')),
    version_constraint TEXT,
    is_satisfied INTEGER DEFAULT 0,
    FOREIGN KEY (package_id) REFERENCES packages(id) ON DELETE CASCADE
);

-- Installation history (audit trail)
CREATE TABLE IF NOT EXISTS install_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    package_name TEXT NOT NULL,
    version TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('install', 'upgrade', 'reinstall', 'uninstall', 'link', 'unlink')),
    performed_at INTEGER NOT NULL,
    success INTEGER NOT NULL,
    error_message TEXT
);

-- Indices for fast lookups
CREATE INDEX IF NOT EXISTS idx_packages_name ON packages(name);
CREATE INDEX IF NOT EXISTS idx_packages_tap ON packages(tap);
CREATE INDEX IF NOT EXISTS idx_package_files_package ON package_files(package_id);
CREATE INDEX IF NOT EXISTS idx_package_deps_package ON package_dependencies(package_id);
CREATE INDEX IF NOT EXISTS idx_package_deps_name ON package_dependencies(dependency_name);
CREATE INDEX IF NOT EXISTS idx_install_history_package ON install_history(package_name);
CREATE INDEX IF NOT EXISTS idx_install_history_time ON install_history(performed_at);
