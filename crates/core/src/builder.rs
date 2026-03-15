//! Build command executor
//!
//! Runs a formula's `[build]` commands with variable substitution and
//! environment injection. All commands are executed via `sh -c`.

use anyhow::{bail, Context, Result};
use brew_formula::Formula;
use std::path::Path;
use std::process::{Command, Stdio};
use tracing::info;

/// Run each command from `formula.build.commands` in sequence via `sh -c`.
///
/// `source_dir` is the working directory. `prefix` becomes `$PREFIX`,
/// `cellar_root` becomes `$CELLAR`. Fails on the first non-zero exit.
pub fn run_build(formula: &Formula, source_dir: &Path, prefix: &Path, cellar_root: &Path) -> Result<()> {
    let ncpu = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .to_string();

    let prefix_str = prefix.to_string_lossy();
    let cellar_str = cellar_root.to_string_lossy();
    let name = formula.name();
    let version = formula.version();

    let commands = &formula.build.commands;
    if commands.is_empty() {
        info!("No build commands for {}", name);
        return Ok(());
    }

    for raw_cmd in commands {
        let cmd = substitute_vars(raw_cmd, &prefix_str, &ncpu, version, name, &cellar_str);
        info!("  Running: {}", cmd);

        let mut proc = Command::new("sh");
        proc.arg("-c")
            .arg(&cmd)
            .current_dir(source_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Inject formula build env (with variable substitution)
        for (key, val) in &formula.build.env {
            let val_subst = substitute_vars(val, &prefix_str, &ncpu, version, name, &cellar_str);
            proc.env(key, val_subst);
        }

        let output = proc
            .output()
            .with_context(|| format!("Failed to spawn command: {}", cmd))?;

        if !output.status.success() {
            let combined = combine_output(&output.stdout, &output.stderr);
            let tail = last_n_lines(&combined, 20);
            bail!(
                "Build command failed: {}\n\nLast output:\n{}",
                cmd,
                tail
            );
        }
    }

    Ok(())
}

/// Expand $PREFIX, $CELLAR, $NCPU, $VERSION, $NAME in a string.
fn substitute_vars(s: &str, prefix: &str, ncpu: &str, version: &str, name: &str, cellar: &str) -> String {
    s.replace("$PREFIX", prefix)
        .replace("$CELLAR", cellar)
        .replace("$NCPU", ncpu)
        .replace("$VERSION", version)
        .replace("$NAME", name)
}

/// Concatenate stdout and stderr into one string.
fn combine_output(stdout: &[u8], stderr: &[u8]) -> String {
    let mut out = String::new();
    if !stdout.is_empty() {
        out.push_str(&String::from_utf8_lossy(stdout));
    }
    if !stderr.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&String::from_utf8_lossy(stderr));
    }
    out
}

/// Return the last `n` lines of a string.
fn last_n_lines(s: &str, n: usize) -> &str {
    let lines: Vec<&str> = s.lines().collect();
    if lines.len() <= n {
        return s;
    }
    let start_line = lines.len() - n;
    // Find byte offset of the start line
    let mut byte_offset = 0;
    let mut line_count = 0;
    for (i, ch) in s.char_indices() {
        if line_count == start_line {
            byte_offset = i;
            break;
        }
        if ch == '\n' {
            line_count += 1;
        }
    }
    &s[byte_offset..]
}

#[cfg(test)]
mod tests {
    use super::*;
    use brew_formula::Formula;
    use std::fs;
    use tempfile::TempDir;

    fn make_formula_with_build(commands: Vec<&str>, env: Vec<(&str, &str)>) -> Formula {
        let env_section = if env.is_empty() {
            String::new()
        } else {
            let pairs: Vec<String> = env
                .iter()
                .map(|(k, v)| format!("{} = \"{}\"", k, v))
                .collect();
            format!("\n[build.env]\n{}", pairs.join("\n"))
        };

        let cmds: Vec<String> = commands.iter().map(|c| format!("\"{}\"", c)).collect();

        let toml = format!(
            r#"
[package]
name = "test-pkg"
version = "1.0.0"
description = "Test"

[source]
url = "https://example.com/test.tar.gz"
sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

[build]
commands = [{}]
{}
"#,
            cmds.join(", "),
            env_section
        );

        Formula::from_str_unchecked(&toml).unwrap()
    }

    #[test]
    fn test_variable_substitution() {
        let result = substitute_vars(
            "./configure --prefix=$PREFIX -j$NCPU",
            "/usr/local",
            "4",
            "1.0.0",
            "mypkg",
            "/cellar",
        );
        assert_eq!(result, "./configure --prefix=/usr/local -j4");
    }

    #[test]
    fn test_version_and_name_substitution() {
        let result = substitute_vars(
            "echo $NAME-$VERSION",
            "/prefix",
            "8",
            "2.1.0",
            "curl",
            "/cellar",
        );
        assert_eq!(result, "echo curl-2.1.0");
    }

    #[test]
    fn test_cellar_substitution() {
        let result = substitute_vars(
            "--with-openssl=$CELLAR/openssl/3.0",
            "/prefix",
            "4",
            "1.0.0",
            "curl",
            "/home/user/cellar",
        );
        assert_eq!(result, "--with-openssl=/home/user/cellar/openssl/3.0");
    }

    #[test]
    fn test_run_build_creates_file() {
        let tmp = TempDir::new().unwrap();
        let source_dir = tmp.path().join("src");
        let prefix = tmp.path().join("prefix");
        let cellar = tmp.path().join("cellar");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&prefix).unwrap();

        let formula = make_formula_with_build(
            vec!["touch $PREFIX/built.txt"],
            vec![],
        );

        run_build(&formula, &source_dir, &prefix, &cellar).unwrap();
        assert!(prefix.join("built.txt").exists(), "build should have created built.txt");
    }

    #[test]
    fn test_run_build_injects_env() {
        let tmp = TempDir::new().unwrap();
        let source_dir = tmp.path().join("src");
        let prefix = tmp.path().join("prefix");
        let cellar = tmp.path().join("cellar");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&prefix).unwrap();

        let formula = make_formula_with_build(
            vec!["printenv CC > $PREFIX/compiler.txt"],
            vec![("CC", "clang")],
        );

        run_build(&formula, &source_dir, &prefix, &cellar).unwrap();

        let content = fs::read_to_string(prefix.join("compiler.txt")).unwrap();
        assert_eq!(content.trim(), "clang");
    }

    #[test]
    fn test_run_build_failure_shows_output() {
        let tmp = TempDir::new().unwrap();
        let source_dir = tmp.path().join("src");
        let prefix = tmp.path().join("prefix");
        let cellar = tmp.path().join("cellar");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&prefix).unwrap();

        let formula = make_formula_with_build(
            vec!["echo 'error output'; exit 1"],
            vec![],
        );

        let err = run_build(&formula, &source_dir, &prefix, &cellar).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Build command failed"), "error should mention failed command");
        assert!(msg.contains("error output"), "error should contain command output");
    }

    #[test]
    fn test_run_build_empty_commands() {
        let tmp = TempDir::new().unwrap();
        let source_dir = tmp.path().join("src");
        let prefix = tmp.path().join("prefix");
        let cellar = tmp.path().join("cellar");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&prefix).unwrap();

        let formula = make_formula_with_build(vec![], vec![]);
        run_build(&formula, &source_dir, &prefix, &cellar).unwrap();
    }

    #[test]
    fn test_last_n_lines() {
        let s = "line1\nline2\nline3\nline4\nline5";
        let tail = last_n_lines(s, 3);
        assert!(tail.contains("line3"));
        assert!(tail.contains("line5"));
        assert!(!tail.contains("line1"));
    }
}
