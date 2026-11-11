//! Records the short git SHA of the build, so that `seek --version` can print it (SPEC 2.4).
//!
//! The SHA is optional: a build from a tarball without a git checkout still works, it just has no
//! SHA to show.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../.git/HEAD");

    let sha = git_sha();
    match sha {
        Some(sha) => {
            println!("cargo:rustc-env=SEEK_GIT_SHA={sha}");
        }
        None => {
            println!("cargo:rustc-env=SEEK_GIT_SHA=unknown");
        }
    }
}

/// The short SHA of the current commit, if there is one.
fn git_sha() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?;
    let sha = sha.trim();
    if sha.is_empty() {
        None
    } else {
        Some(sha.to_string())
    }
}
