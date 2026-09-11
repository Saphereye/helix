use std::path::Path;
use std::process::Command;

const MAJOR: &str = env!("CARGO_PKG_VERSION_MAJOR");
const MINOR: &str = env!("CARGO_PKG_VERSION_MINOR");
const PATCH: &str = env!("CARGO_PKG_VERSION_PATCH");

fn main() {
    let version = std::env::var("HELIX_PKGVER")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(fork_version_from_git)
        .unwrap_or_else(fallback_version);

    println!(
        "cargo:rustc-env=BUILD_TARGET={}",
        std::env::var("TARGET").unwrap()
    );
    println!("cargo:rustc-env=VERSION_AND_GIT_HASH={version}");

    register_git_rerun_if_changed();
}

/// Torch-style: `25.07.1-20260906-d47f0771` — calver, commit date, short hash.
fn fork_version_from_git() -> Option<String> {
    let ver = calver();
    let date = git_output(&["log", "-1", "--format=%cs", "HEAD"])?.replace('-', "");
    let hash = git_output(&["rev-parse", "--short=8", "HEAD"])?;
    Some(format!("{ver}-{date}-{hash}"))
}

fn calver() -> String {
    let minor: u32 = MINOR.parse().unwrap_or(0);
    format!("{MAJOR}.{minor:02}.{PATCH}")
}

fn fallback_version() -> String {
    calver()
}

fn git_output(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn register_git_rerun_if_changed() {
    let cargo_toml = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");
    if cargo_toml.exists() {
        println!("cargo:rerun-if-changed={}", cargo_toml.display());
    }

    if git_output(&["rev-parse", "HEAD"]).is_none()
        && option_env!("HELIX_NIX_BUILD_REV").is_none()
    {
        return;
    }

    let Some(git_dir) = git_output(&["rev-parse", "--git-dir"]) else {
        return;
    };

    let head = Path::new(&git_dir).join("HEAD");
    if head.exists() {
        println!("cargo:rerun-if-changed={}", head.display());
    }

    let Some(head_ref) = git_output(&["symbolic-ref", "HEAD"]) else {
        return;
    };
    let head_ref = Path::new(&git_dir).join(head_ref);
    if head_ref.exists() {
        println!("cargo:rerun-if-changed={}", head_ref.display());
    }
}
