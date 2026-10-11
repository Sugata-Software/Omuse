//! Write candidate colour references to a NEW review directory. Never updates
//! test references or accepts an existing directory. See reference/photo-color.
#[path = "../tests/reference/photo-color/corpus.rs"]
mod corpus;

use anyhow::{Context, Result, ensure};
use omuse::{asset_library::sha256_hex, camera_raw};
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let output = PathBuf::from(
        args.next()
            .context("Usage: photo_color_baseline NEW_REVIEW_DIRECTORY")?,
    );
    ensure!(
        args.next().is_none(),
        "Expected exactly one new review directory"
    );
    // create_dir deliberately fails if the destination exists, including an
    // existing empty directory. There is no --force or baseline-bless option.
    fs::create_dir(&output).context("Review directory must not already exist")?;
    let source = corpus::fixture();
    source.save(output.join("input.png"))?;
    let mut references = Vec::new();
    for case in corpus::cases() {
        ensure!(
            case.name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'_'),
            "Invalid reference name"
        );
        let result = camera_raw::apply(&source, &case.settings)?;
        let file = format!("{}.png", case.name);
        result.save(output.join(&file))?;
        references.push(json!({
            "name":case.name, "purpose":case.purpose, "file":file,
            "pixelsSha256":sha256_hex(result.as_raw()),
            "pngSha256":sha256_hex(&fs::read(output.join(&file))?),
        }));
    }
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    let manifest = json!({
        "schema":1,
        "origin":"Omuse camera_raw appearance baseline; not Adobe parity or a colour-accuracy certification",
        "sourceRevisionContext":revision,
        "revisionIsContextOnly":true,
        "omuseVersion":env!("CARGO_PKG_VERSION"),
        "platform":{"os":std::env::consts::OS,"arch":std::env::consts::ARCH},
        "cameraRawSourceSha256":sha256_hex(include_bytes!("../src/camera_raw.rs")),
        "generatorSourceSha256":sha256_hex(include_bytes!("photo_color_baseline.rs")),
        "fixtureDefinitionSha256":sha256_hex(include_bytes!("../tests/reference/photo-color/corpus.rs")),
        "caseDefinitionsSha256":sha256_hex(include_bytes!("../tests/reference/photo-color/cases.json")),
        "width":corpus::WIDTH, "height":corpus::HEIGHT, "rows":corpus::ROWS,
        "inputPixelsSha256":sha256_hex(source.as_raw()),
        "inputPngSha256":sha256_hex(&fs::read(output.join("input.png"))?),
        "references":references,
        "review":"Candidate files require explicit review and manual promotion; tests never generate or bless references.",
    });
    fs::write(
        output.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    println!(
        "Wrote {} candidate references to {}",
        references.len(),
        output.display()
    );
    Ok(())
}
