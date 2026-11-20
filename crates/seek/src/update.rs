//! `seek update`: install the latest release of `main` (SPEC 13).

use anyhow::{Context, Result, bail};

const REPO: &str = "https://github.com/piskevalee-cpu/Seek";
const RAW_CARGO: &str = "https://raw.githubusercontent.com/piskevalee-cpu/Seek/main/Cargo.toml";

/// The release target of this machine, using the same names as the release workflow.
pub fn target() -> String {
    let arch = std::env::consts::ARCH;
    let os = match std::env::consts::OS {
        "linux" => "unknown-linux-gnu",
        "macos" => "apple-darwin",
        other => other,
    };
    format!("{arch}-{os}")
}

/// Compare `major.minor.patch` versions, ignoring any suffix.
pub fn newer(latest: &str, current: &str) -> bool {
    let parse = |value: &str| -> Option<(u64, u64, u64)> {
        let numbers: Vec<u64> = value
            .split('.')
            .take(3)
            .map(|part| {
                part.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
            })
            .map(|part| part.parse().unwrap_or(0))
            .collect();
        Some((
            numbers.first().copied()?,
            numbers.get(1).copied()?,
            numbers.get(2).copied()?,
        ))
    };
    match (parse(latest), parse(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// The version in the workspace manifest of `main`.
async fn latest_version(client: &reqwest::Client) -> Result<String> {
    let text = client
        .get(RAW_CARGO)
        .send()
        .await
        .context("could not reach the SEEK repository")?
        .error_for_status()
        .context("could not read the manifest of main")?
        .text()
        .await
        .context("could not read the response")?;
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("version = \"")
            && let Some(version) = rest.split('"').next()
        {
            return Ok(version.to_string());
        }
    }
    bail!("no version found in the manifest of main")
}

/// Find the `seek` executable in an extracted release.
fn find_binary(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().is_some_and(|name| name == "seek") {
            return Some(path);
        }
    }
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        if entry.path().is_dir()
            && let Some(found) = find_binary(&entry.path())
        {
            return Some(found);
        }
    }
    None
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

/// Replace the running binary. On Unix a rename over the old path is atomic; Windows cannot
/// overwrite a running executable, so it reports the downloaded path instead.
fn replace_binary(source: &std::path::Path) -> Result<std::path::PathBuf> {
    let current = std::env::current_exe().context("could not find the running executable")?;
    let next = current.with_extension("next");
    std::fs::copy(source, &next).context("could not stage the new binary")?;
    make_executable(&next)?;
    #[cfg(unix)]
    {
        std::fs::rename(&next, &current).context("could not replace the running binary")?;
        Ok(current)
    }
    #[cfg(not(unix))]
    {
        Ok(next)
    }
}

/// Check for a newer release and, unless `check` is set, install it.
pub async fn run(check: bool) -> Result<()> {
    let client = reqwest::Client::builder()
        .user_agent(format!("seek/{}", env!("CARGO_PKG_VERSION")))
        .build()?;
    let current = env!("CARGO_PKG_VERSION");
    println!("current   {current}");
    println!("checking  main");
    let latest = latest_version(&client).await?;
    println!("latest    {latest}");
    if !newer(&latest, current) {
        println!("SEEK is up to date.");
        return Ok(());
    }
    if check {
        println!("Run `seek update` to install {latest}.");
        return Ok(());
    }

    let target = target();
    let asset = format!("seek-v{latest}-{target}.tar.gz");
    let url = format!("{REPO}/releases/download/v{latest}/{asset}");
    println!("download  {url}");
    let bytes = client
        .get(&url)
        .send()
        .await
        .context("could not reach the release")?
        .error_for_status()
        .with_context(|| format!("no release asset for {target} yet"))?
        .bytes()
        .await
        .context("could not download the release")?;

    let temp = std::env::temp_dir().join(format!("seek-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp)?;
    let archive = temp.join(&asset);
    std::fs::write(&archive, &bytes)?;
    let status = std::process::Command::new("tar")
        .args(["-xzf"])
        .arg(&archive)
        .arg("-C")
        .arg(&temp)
        .status()
        .context("tar is needed to unpack the release")?;
    if !status.success() {
        bail!("could not unpack the release");
    }
    let binary = find_binary(&temp).context("the release has no seek binary")?;
    let installed = replace_binary(&binary)?;
    let _ = std::fs::remove_dir_all(&temp);

    println!("installed {latest} at {}", installed.display());
    seek_tui::notify::notify("SEEK", &format!("Updated to {latest}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(newer("0.7.28", "0.7.27"));
        assert!(newer("0.8.0", "0.7.28"));
        assert!(!newer("0.7.27", "0.7.27"));
        assert!(!newer("0.7.26", "0.7.27"));
    }

    #[test]
    fn target_names_the_release_platform() {
        let target = target();
        assert!(target.contains('-'));
        assert!(target.starts_with("x86_64") || target.starts_with("aarch64"));
    }
}
