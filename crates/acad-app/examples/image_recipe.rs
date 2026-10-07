//! Convert a pinned PNG into a recipe accepted by the existing artwork exporter.
use acad_app::art_image::{convert, recipe_bytes, Config};
use std::path::Path;
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(2..=3).contains(&args.len()) || args.get(2).is_some_and(|a| a != "--check") {
        return Err("usage: image_recipe CONFIG.json OUTPUT.json [--check]".into());
    }
    generate(Path::new(&args[0]), Path::new(&args[1]), args.len() == 3)
}
fn generate(config_path: &Path, output: &Path, check: bool) -> Result<(), String> {
    let bytes = std::fs::read(config_path).map_err(|e| e.to_string())?;
    let config: Config = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let input_path = acad_app::art::relative_path(
        config_path.parent().unwrap_or(Path::new(".")),
        &config.image,
    )?;
    let target = if output.exists() {
        output.canonicalize()
    } else {
        output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()
            .map(|p| p.join(output.file_name().unwrap_or_default()))
    }
    .map_err(|e| e.to_string())?;
    for p in [config_path, &input_path] {
        if p.canonicalize()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .to_lowercase()
            == target.to_string_lossy().to_lowercase()
        {
            return Err("output aliases conversion input".into());
        }
    }
    if std::fs::metadata(&input_path)
        .map_err(|e| e.to_string())?
        .len()
        > acad_app::art_image::MAX_INPUT_BYTES as u64
    {
        return Err("PNG exceeds 16 MiB input limit".into());
    }
    let image = std::fs::read(input_path).map_err(|e| e.to_string())?;
    let (recipe, report) = convert(&config, &image)?;
    let bytes = recipe_bytes(&recipe)?;
    if check {
        if std::fs::read(output).map_err(|e| e.to_string())? != bytes {
            return Err("converted recipe differs; regenerate".into());
        }
    } else {
        std::fs::write(output, bytes).map_err(|e| e.to_string())?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn file_conversion_preserves_inputs_and_check_never_writes() {
        let root = std::env::temp_dir().join(format!("autorust-image-cli-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut bytes = Vec::new();
        {
            let mut e = png::Encoder::new(&mut bytes, 4, 4);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            e.write_header()
                .unwrap()
                .write_image_data(&[150; 48])
                .unwrap();
        }
        let input = root.join("input.png");
        std::fs::write(&input, &bytes).unwrap();
        let config = Config {
            schema_version: 1,
            id: "cli-test".into(),
            image: "input.png".into(),
            image_sha256: format!("{:x}", Sha256::digest(&bytes)),
            columns: 4,
            colors: 2,
            contours: false,
            contour_color: 250,
            contrast_threshold: 0,
            matte: [255; 3],
        };
        let cfg = root.join("config.json");
        std::fs::write(&cfg, serde_json::to_vec(&config).unwrap()).unwrap();
        let out = root.join("recipe.json");
        assert!(generate(&cfg, &input, false)
            .unwrap_err()
            .contains("aliases"));
        assert_eq!(std::fs::read(&input).unwrap(), bytes);
        assert!(generate(&cfg, &out, true).is_err());
        assert!(!out.exists());
        generate(&cfg, &out, false).unwrap();
        generate(&cfg, &out, true).unwrap();
        std::fs::write(&out, b"changed").unwrap();
        assert!(generate(&cfg, &out, true).unwrap_err().contains("differs"));
        assert_eq!(std::fs::read(&out).unwrap(), b"changed");
        let mut bad = config;
        bad.image = "../escape.png".into();
        std::fs::write(&cfg, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(generate(&cfg, &out, false).unwrap_err().contains("unsafe"));
        assert_eq!(std::fs::read(&out).unwrap(), b"changed");
        std::fs::remove_dir_all(root).unwrap();
    }
}
