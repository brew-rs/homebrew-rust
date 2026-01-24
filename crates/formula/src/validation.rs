//! Formula validation

use crate::Formula;
use anyhow::Result;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Package name is empty")]
    EmptyName,

    #[error("Package name contains invalid characters: {0}")]
    InvalidNameFormat(String),

    #[error("Package version is empty")]
    EmptyVersion,

    #[error("Invalid semantic version: {0}")]
    InvalidVersion(String),

    #[error("Package description is empty")]
    EmptyDescription,

    #[error("Source URL is empty")]
    EmptySourceUrl,

    #[error("Invalid URL format: {0}")]
    InvalidUrl(String),

    #[error("SHA-256 checksum is empty")]
    EmptySha256,

    #[error("Invalid SHA-256 format (expected 64 hex characters): {0}")]
    InvalidSha256(String),

    #[error("Invalid email format in maintainers: {0}")]
    InvalidEmail(String),
}

/// Validate a complete formula
pub fn validate_formula(formula: &Formula) -> Result<()> {
    validate_package(&formula.package)?;
    validate_source(&formula.source)?;
    validate_dependencies(&formula.dependencies)?;
    Ok(())
}

/// Validate package section
fn validate_package(package: &crate::PackageInfo) -> Result<()> {
    // Name validation
    if package.name.is_empty() {
        return Err(ValidationError::EmptyName.into());
    }

    validate_package_name(&package.name)?;

    // Version validation
    if package.version.is_empty() {
        return Err(ValidationError::EmptyVersion.into());
    }

    validate_semver(&package.version)?;

    // Description validation
    if package.description.is_empty() {
        return Err(ValidationError::EmptyDescription.into());
    }

    // Homepage URL validation (if present)
    if let Some(ref homepage) = package.homepage {
        validate_url(homepage)?;
    }

    // Maintainer email validation
    for email in &package.maintainers {
        validate_email(email)?;
    }

    Ok(())
}

/// Validate source section
fn validate_source(source: &crate::SourceInfo) -> Result<()> {
    // URL validation
    if source.url.is_empty() {
        return Err(ValidationError::EmptySourceUrl.into());
    }

    validate_url(&source.url)?;

    // SHA-256 validation
    if source.sha256.is_empty() {
        return Err(ValidationError::EmptySha256.into());
    }

    validate_sha256(&source.sha256)?;

    // Mirror URL validation
    for mirror in &source.mirrors {
        validate_url(mirror)?;
    }

    Ok(())
}

/// Validate dependencies section
fn validate_dependencies(_deps: &crate::Dependencies) -> Result<()> {
    // TODO: Validate dependency names and version constraints
    // For now, just accept any strings
    Ok(())
}

/// Validate package name format (lowercase, alphanumeric + hyphens)
fn validate_package_name(name: &str) -> Result<()> {
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(ValidationError::InvalidNameFormat(name.to_string()).into());
    }

    // Must start with a letter
    if !name.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false) {
        return Err(ValidationError::InvalidNameFormat(
            "Name must start with a lowercase letter".to_string(),
        ).into());
    }

    Ok(())
}

/// Validate semantic version format
fn validate_semver(version: &str) -> Result<()> {
    if let Err(e) = semver::Version::parse(version) {
        return Err(ValidationError::InvalidVersion(e.to_string()).into());
    }
    Ok(())
}

/// Validate URL format
fn validate_url(url_str: &str) -> Result<()> {
    match url::Url::parse(url_str) {
        Ok(url) => {
            // Must be http or https
            if url.scheme() != "http" && url.scheme() != "https" {
                return Err(ValidationError::InvalidUrl(
                    "URL must use http or https scheme".to_string(),
                ).into());
            }
            Ok(())
        }
        Err(e) => Err(ValidationError::InvalidUrl(e.to_string()).into()),
    }
}

/// Validate SHA-256 checksum format (64 hexadecimal characters)
fn validate_sha256(sha256: &str) -> Result<()> {
    if sha256.len() != 64 {
        return Err(ValidationError::InvalidSha256(
            format!("Expected 64 characters, got {}", sha256.len()),
        ).into());
    }

    if !sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ValidationError::InvalidSha256(
            "Contains non-hexadecimal characters".to_string(),
        ).into());
    }

    Ok(())
}

/// Validate email format (basic check)
fn validate_email(email: &str) -> Result<()> {
    // Must contain @ with non-empty parts before and after
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(ValidationError::InvalidEmail(email.to_string()).into());
    }

    // Domain part must contain a dot
    if !parts[1].contains('.') {
        return Err(ValidationError::InvalidEmail(email.to_string()).into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_package_name_valid() {
        assert!(validate_package_name("curl").is_ok());
        assert!(validate_package_name("node-js").is_ok());
        assert!(validate_package_name("python3").is_ok());
    }

    #[test]
    fn test_validate_package_name_invalid() {
        assert!(validate_package_name("Curl").is_err()); // Uppercase
        assert!(validate_package_name("curl_rs").is_err()); // Underscore
        assert!(validate_package_name("123curl").is_err()); // Starts with number
        assert!(validate_package_name("curl@8").is_err()); // Special char
    }

    #[test]
    fn test_validate_semver_valid() {
        assert!(validate_semver("1.0.0").is_ok());
        assert!(validate_semver("2.3.4-alpha").is_ok());
        assert!(validate_semver("1.2.3+build.123").is_ok());
    }

    #[test]
    fn test_validate_semver_invalid() {
        assert!(validate_semver("1.0").is_err());
        assert!(validate_semver("v1.0.0").is_err());
        assert!(validate_semver("abc").is_err());
    }

    #[test]
    fn test_validate_url_valid() {
        assert!(validate_url("https://example.com/file.tar.gz").is_ok());
        assert!(validate_url("http://mirror.org/package.zip").is_ok());
    }

    #[test]
    fn test_validate_url_invalid() {
        assert!(validate_url("ftp://example.com/file.tar.gz").is_err()); // Wrong scheme
        assert!(validate_url("not-a-url").is_err());
        assert!(validate_url("").is_err());
    }

    #[test]
    fn test_validate_sha256_valid() {
        assert!(validate_sha256("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855").is_ok());
        assert!(validate_sha256("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef").is_ok());
    }

    #[test]
    fn test_validate_sha256_invalid() {
        assert!(validate_sha256("abc123").is_err()); // Too short
        assert!(validate_sha256("not-hex-characters-here-not-hex-characters-here-not-hex-char").is_err());
        assert!(validate_sha256("").is_err());
    }

    #[test]
    fn test_validate_email_valid() {
        assert!(validate_email("user@example.com").is_ok());
        assert!(validate_email("name.surname@company.co.uk").is_ok());
    }

    #[test]
    fn test_validate_email_invalid() {
        assert!(validate_email("notanemail").is_err());
        assert!(validate_email("missing@domain").is_err());
        assert!(validate_email("@example.com").is_err());
    }
}
