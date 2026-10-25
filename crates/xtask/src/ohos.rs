//! OHOS embedding SDK cross compilation and paired source metadata.

use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::Path,
    process::Command,
};

use anyhow::{Context, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::term;

pub(crate) fn build(release: bool, dev: bool, mut features: Vec<String>) -> anyhow::Result<()> {
    features.sort();
    features.dedup();
    ensure!(
        features
            .iter()
            .all(|f| matches!(f.as_str(), "api-23" | "api-26")),
        "OHOS features must be api-23 or api-26"
    );
    let minimum_api = if features.iter().any(|f| f == "api-26") {
        26
    } else if features.iter().any(|f| f == "api-23") {
        23
    } else {
        20
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let profile = if release { "release" } else { "debug" };
    let profile = if dev {
        format!("{profile}-dev")
    } else {
        profile.into()
    };
    let profile = if features.is_empty() {
        profile
    } else {
        format!("{profile}-{}", features.join("-"))
    };
    let out = root.join("target/ohos").join(&profile);
    if dev {
        features.push("dev".into());
    }
    // Resolve the same locked feature graph once, before invoking the builder.
    let mut metadata = Command::new("cargo");
    metadata.current_dir(&root).args([
        "metadata",
        "--locked",
        "--format-version",
        "1",
        "--filter-platform",
        "aarch64-unknown-linux-ohos",
    ]);
    if !features.is_empty() {
        metadata.args([
            "--features",
            &features
                .iter()
                .map(|f| format!("nsh-platform-ohos/{f}"))
                .collect::<Vec<_>>()
                .join(","),
        ]);
    }
    let metadata = metadata
        .output()
        .context("read locked OHOS dependency graph")?;
    ensure!(
        metadata.status.success(),
        "Cargo metadata failed: {}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata: Value = serde_json::from_slice(&metadata.stdout)?;
    let packages = metadata["packages"]
        .as_array()
        .context("Cargo package inventory")?;
    let ability = packages
        .iter()
        .find(|p| p["name"] == "openharmony-ability")
        .context("Ability dependency")?;
    let bindings: BTreeSet<_> = packages
        .iter()
        .filter(|p| {
            p["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("ohos-") && name != "ohos-enum-derive")
        })
        .map(|p| p["source"].as_str().unwrap_or("path"))
        .collect();
    ensure!(
        bindings.len() == 1,
        "OHOS native packages must share one binding source: {bindings:?}"
    );
    let ability_source = ability["source"].as_str().unwrap_or("path");
    let binding_source = *bindings.first().context("OHOS bindings")?;
    let path_development = std::env::var_os("NSH_ALLOW_PATH_DEPENDENCIES").is_some();
    ensure!(
        path_development
            || (ability_source.starts_with("git+") && binding_source.starts_with("git+")),
        "SDK dependencies must use pinned Ability and binding Git revisions"
    );
    let step = term::Step::start("building OHOS embedding SDK");
    let mut command = Command::new("ohrs");
    command
        .current_dir(&root)
        .args(["build", "-p", "nsh-platform-ohos", "-a", "aarch", "--dist"])
        .arg(&out)
        .arg("--target-dir")
        .arg(root.join("target"));
    if release {
        command.arg("--release");
    }
    command.arg("--").arg("--locked");
    if !features.is_empty() {
        command.args(["--features", &features.join(",")]);
    }
    term::run(&mut command)?;
    let library = out.join("arm64-v8a/libnsh_platform_ohos.so");
    let declarations = out.join("index.d.ts");
    let ndk = std::env::var_os("OHOS_NDK_HOME").context("OHOS_NDK_HOME is required")?;
    term::run(
        Command::new("python3")
            .arg(root.join("examples/embed-ohos/tests/audit-api.py"))
            .arg(&library)
            .arg("--sdk")
            .arg(ndk)
            .arg("--api")
            .arg(minimum_api.to_string())
            .arg("--output")
            .arg(out.join("native-imports.json")),
    )?;
    fs::write(
        out.join("build-metadata.json"),
        serde_json::to_vec_pretty(&json!({
            "minimumApi": minimum_api, "features": features, "profile": profile,
            "abilitySource": ability_source, "bindingSource": binding_source,
            "abilityGitRevision": ability_source.rsplit('#').next().filter(|_| ability_source.starts_with("git+")),
            "bindingGitRevision": binding_source.rsplit('#').next().filter(|_| binding_source.starts_with("git+")),
            "librarySha256": sha256(&library)?, "declarationsSha256": sha256(&declarations)?,
            "pathDevelopment": path_development,
        }))?,
    )?;
    step.done(format!(
        "OHOS API {minimum_api} embedding SDK: {}",
        out.display()
    ));
    Ok(())
}

fn sha256(path: &Path) -> anyhow::Result<String> {
    let mut reader = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
