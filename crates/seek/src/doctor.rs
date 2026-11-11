//! The checks of `seek doctor` (SPEC 13).

use std::path::Path;

use seek_core::ProvidersConfig;

/// Report what SEEK found.
///
/// The check is the same one the install script does: the home folder, the providers and their
/// keys, and the programs SEEK needs.
pub fn doctor(home: &Path, providers: &ProvidersConfig) -> anyhow::Result<()> {
    println!("SEEK {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("home            {}", home.display());
    println!(
        "providers.toml  {}",
        if home.join(seek_core::config::PROVIDERS_FILE).exists() {
            "found"
        } else {
            "missing"
        }
    );

    let mut configured = 0;
    for (id, provider) in &providers.provider {
        let key = provider.api_key();
        let state = if key.is_some() { "key found" } else { "no key" };
        println!(
            "  {id:<16} {:<20} {state}",
            provider.kind.clone().unwrap_or_default()
        );
        configured += 1;
    }
    if configured == 0 {
        println!("  no providers configured");
    }

    let models = home.join(seek_local::MODELS_DIR);
    let local = seek_local::model::scan(&models);
    println!();
    println!(
        "local models    {}",
        if local.is_empty() {
            "none in ~/seek/models".to_string()
        } else {
            local
                .iter()
                .map(|model| model.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );

    println!();
    println!(
        "git             {}",
        crate::which("git")
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "not found".to_string())
    );
    println!(
        "llama server    {}",
        crate::which("llama")
            .or_else(|| crate::which("llama-server"))
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "not found".to_string())
    );

    Ok(())
}
