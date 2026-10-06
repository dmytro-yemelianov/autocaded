//! Deterministically generate editable artwork and geometry-only previews.
use acad_app::art::*;
use acad_render::{FrameBudget, Libraries, Prim, Viewport};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn path(base: &Path, p: &str) -> Result<PathBuf, String> {
    if p.is_empty()
        || p.contains('\\')
        || p.split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || Path::new(p)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("unsafe catalog path {p}"));
    }
    Ok(base.join(p))
}
fn collision_key(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}
fn svg(ps: &[Prim]) -> Vec<u8> {
    let mut s=String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"800\" height=\"600\" viewBox=\"0 0 800 600\"><rect width=\"800\" height=\"600\" fill=\"black\"/>");
    for p in ps {
        let (pts, rgb, fill) = match p {
            Prim::Polyline(p) => (p, [255, 255, 255], false),
            Prim::ColoredPolyline { points, rgb } => (points, *rgb, false),
            Prim::FilledPolygon(p) => (p, [255, 255, 255], true),
            Prim::ColoredFilledPolygon { points, rgb } => (points, *rgb, true),
        };
        let pts = pts
            .iter()
            .map(|p| format!("{},{}", p.x, p.y))
            .collect::<Vec<_>>()
            .join(" ");
        let color = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
        if fill {
            s.push_str(&format!("<polygon points=\"{pts}\" fill=\"{color}\"/>"));
        } else {
            s.push_str(&format!(
                "<polyline points=\"{pts}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"1\"/>"
            ));
        }
    }
    s.push_str("</svg>\n");
    s.into_bytes()
}
fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() > 2 || args.get(1).is_some_and(|a| a != "--check") {
        return Err("usage: art_drawings [catalog.json] [--check]".into());
    }
    let catalog_path = Path::new(args.first().map_or("demo/art/catalog.json", String::as_str));
    generate(catalog_path, args.get(1).is_some())
}
fn generate(catalog_path: &Path, check: bool) -> Result<(), String> {
    let base = catalog_path.parent().unwrap_or(Path::new("."));
    let catalog_bytes = std::fs::read(catalog_path).map_err(|e| e.to_string())?;
    let catalog: Catalog = serde_json::from_slice(&catalog_bytes).map_err(|e| e.to_string())?;
    if catalog.schema_version != 1 || catalog.entries.is_empty() {
        return Err("unsupported or empty catalog".into());
    }
    let mut input_paths = std::collections::BTreeSet::new();
    input_paths.insert(collision_key(catalog_path));
    for entry in &catalog.entries {
        input_paths.insert(collision_key(&path(base, &entry.recipe)?));
        input_paths.insert(collision_key(&path(base, &entry.source.snapshot)?));
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeSet::new();
    let mut artifacts = Vec::new();
    let mut manifest = Vec::new();
    for entry in catalog.entries {
        if !valid_id(&entry.id)
            || !ids.insert(entry.id.clone())
            || [
                &entry.title_key,
                &entry.description_key,
                &entry.source.creator,
                &entry.source.license_url,
                &entry.source.license_version,
                &entry.source.attribution,
                &entry.source.adaptation,
            ]
            .iter()
            .any(|s| s.trim().is_empty())
        {
            return Err("invalid catalog identity or missing metadata".into());
        }
        let mut labels = serde_json::Map::new();
        for (name, key) in [
            ("title", &entry.title_key),
            ("description", &entry.description_key),
        ] {
            let id = acad_cmd::messages::resolve_key(key)
                .ok_or_else(|| format!("unknown label key {key}"))?;
            let en = acad_cmd::messages::text(id, acad_cmd::messages::Locale::En)
                .map_err(|e| e.to_string())?;
            let uk = acad_cmd::messages::text(id, acad_cmd::messages::Locale::Uk)
                .map_err(|e| e.to_string())?;
            labels.insert(name.into(), json!({"key":key,"en":en,"uk":uk}));
        }
        let source =
            std::fs::read(path(base, &entry.source.snapshot)?).map_err(|e| e.to_string())?;
        if hash(&source) != entry.source.sha256 {
            return Err(format!("{} source hash mismatch", entry.id));
        }
        let recipe_bytes = std::fs::read(path(base, &entry.recipe)?).map_err(|e| e.to_string())?;
        let recipe = Recipe::parse(&recipe_bytes)?;
        if recipe.id != entry.id {
            return Err("recipe/catalog id mismatch".into());
        }
        let drawing = recipe.compile()?;
        let vp = Viewport::fit(&drawing.header.extents, 800, 600);
        let mut libs = Libraries::default();
        libs.set_palette(acad_model::Palette::Aci256);
        let mut budget = FrameBudget::default();
        let rendered = acad_render::flatten_with_budget(&drawing, &vp, &libs, &mut budget);
        if rendered.incomplete
            || !rendered.diagnostics.is_empty()
            || rendered.primitives.len() > MAX_PRIMITIVES
        {
            return Err(format!(
                "{} render incomplete/over budget: {:?}",
                entry.id, rendered.diagnostics
            ));
        }
        let png = acad_render::rasterize(&rendered.primitives, 800, 600)
            .encode_png()
            .map_err(|e| e.to_string())?;
        let dwg = acad_dwg::write_version(&drawing, acad_dwg::header::Version::Ac140)
            .map_err(|e| e.to_string())?;
        let dxf = acad_dxf::try_write(&drawing).map_err(|e| e.to_string())?;
        // Exercise both codecs before publishing any output.
        for roundtrip in [
            acad_dwg::parse(&dwg).map_err(|e| e.to_string())?,
            acad_dxf::parse(&dxf).map_err(|e| e.to_string())?,
        ] {
            if roundtrip.items != drawing.items {
                return Err(format!(
                    "codec round-trip changed geometry: first diff {:?}",
                    roundtrip
                        .items
                        .iter()
                        .zip(&drawing.items)
                        .find(|(a, b)| a != b)
                ));
            }
        }
        let mut hashes = serde_json::Map::new();
        for (name, ext, bytes) in [
            (&entry.outputs.dwg, "dwg", dwg),
            (&entry.outputs.dxf, "dxf", dxf),
            (&entry.outputs.svg, "svg", svg(&rendered.primitives)),
            (&entry.outputs.png, "png", png),
        ] {
            if !paths.insert(collision_key(&path(base, name)?))
                || !Path::new(name)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case(ext))
            {
                return Err("duplicate output or wrong extension".into());
            }
            hashes.insert(ext.into(), json!(hash(&bytes)));
            let output_path = path(base, name)?;
            if input_paths.contains(&collision_key(&output_path)) {
                return Err("output collides with authored input".into());
            }
            artifacts.push((output_path, bytes));
        }
        manifest.push(json!({"id":entry.id,"labels":labels,"generator_version":GENERATOR_VERSION,"source_sha256":entry.source.sha256,"recipe_sha256":hash(&recipe_bytes),"entity_count":drawing.items.len(),"layer_count":recipe.layers.len(),"expanded_primitives":rendered.primitives.len(),"render_work":budget.used(),"outputs":entry.outputs,"output_sha256":hashes}));
    }
    let bytes = serde_json::to_vec_pretty(
        &json!({"schema_version":1,"catalog_sha256":hash(&catalog_bytes),"entries":manifest}),
    )
    .map_err(|e| e.to_string())?;
    let manifest_path = path(base, "generated/manifest.json")?;
    if input_paths.contains(&collision_key(&manifest_path)) {
        return Err("manifest collides with authored input".into());
    }
    if artifacts
        .iter()
        .any(|(p, _)| collision_key(p) == collision_key(&manifest_path))
    {
        return Err("reserved manifest path".into());
    }
    artifacts.push((manifest_path, bytes));
    for (p, b) in artifacts {
        if check {
            if std::fs::read(&p).map_err(|e| e.to_string())? != b {
                return Err(format!("{} differs; regenerate", p.display()));
            }
        } else {
            std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::write(&p, b).map_err(|e| e.to_string())?;
        }
        println!(
            "{} {}",
            if check { "checked" } else { "generated" },
            p.display()
        );
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("art_drawings: {e}");
        std::process::exit(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "autorust-art-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn catalog(&self, manifest_source: bool, alias_output: bool) -> PathBuf {
            let mut entries = Vec::new();
            for id in ["first", "second"] {
                let recipe = json!({"schema_version":1,"id":id,"bounds":[0,0,10,10],"layers":[{"number":1,"color":7,"role":"fill"}],"primitives":[{"type":"rectangle","layer":1,"min":[1,1],"max":[9,9],"filled":true}]});
                std::fs::write(
                    self.0.join(format!("{id}.json")),
                    serde_json::to_vec(&recipe).unwrap(),
                )
                .unwrap();
                let snapshot = if manifest_source && id == "first" {
                    "generated/manifest.json".to_string()
                } else {
                    format!("{id}-source.json")
                };
                let source_path = self.0.join(&snapshot);
                std::fs::create_dir_all(source_path.parent().unwrap()).unwrap();
                let source = b"{\"creator\":\"test original\"}";
                std::fs::write(source_path, source).unwrap();
                let dwg = if alias_output {
                    if id == "first" {
                        "generated/shared.DWG".to_string()
                    } else {
                        "generated/./shared.DWG".to_string()
                    }
                } else {
                    format!("generated/{id}.DWG")
                };
                entries.push(json!({"id":id,"category":"colour_study","title_key":"profile.modern.title","description_key":"profile.modern.status","recipe":format!("{id}.json"),"source":{"kind":"original","creator":"test","license_url":"https://example.com/license","license_version":"test","attribution":"test","adaptation":"test","snapshot":snapshot,"sha256":hash(source)},"outputs":{"dwg":dwg,"dxf":format!("generated/{id}.DXF"),"svg":format!("generated/{id}.svg"),"png":format!("generated/{id}.png")}}));
            }
            let p = self.0.join("catalog.json");
            std::fs::write(
                &p,
                serde_json::to_vec(&json!({"schema_version":1,"entries":entries})).unwrap(),
            )
            .unwrap();
            p
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn output_alias_is_rejected_before_any_write() {
        let f = Fixture::new();
        let catalog = f.catalog(false, true);
        let source = std::fs::read(f.0.join("first-source.json")).unwrap();
        assert!(generate(&catalog, false)
            .unwrap_err()
            .contains("unsafe catalog path"));
        assert!(!f.0.join("generated").exists());
        assert_eq!(
            std::fs::read(f.0.join("first-source.json")).unwrap(),
            source
        );
    }
    #[test]
    fn manifest_cannot_replace_source_snapshot() {
        let f = Fixture::new();
        let catalog = f.catalog(true, false);
        let snapshot = f.0.join("generated/manifest.json");
        let source = std::fs::read(&snapshot).unwrap();
        let authored_catalog = std::fs::read(&catalog).unwrap();
        assert!(generate(&catalog, false)
            .unwrap_err()
            .contains("manifest collides with authored input"));
        assert_eq!(std::fs::read(snapshot).unwrap(), source);
        assert_eq!(std::fs::read(catalog).unwrap(), authored_catalog);
        assert!(!f.0.join("generated/first.DWG").exists());
        assert!(!f.0.join("generated/second.DWG").exists());
    }
    #[test]
    fn output_case_alias_is_rejected_before_any_write() {
        let f = Fixture::new();
        let catalog = f.catalog(false, false);
        let mut data: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&catalog).unwrap()).unwrap();
        data["entries"][0]["outputs"]["dwg"] = json!("generated/shared.DWG");
        data["entries"][1]["outputs"]["dwg"] = json!("generated/SHARED.DWG");
        std::fs::write(&catalog, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(generate(&catalog, false)
            .unwrap_err()
            .contains("duplicate output"));
        assert!(!f.0.join("generated").exists());
        assert!(path(&f.0, "generated\\shared.DWG").is_err());
    }
    #[test]
    fn manifest_case_alias_cannot_replace_source() {
        let f = Fixture::new();
        let catalog = f.catalog(false, false);
        let mut data: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&catalog).unwrap()).unwrap();
        let source = std::fs::read(f.0.join("first-source.json")).unwrap();
        std::fs::create_dir_all(f.0.join("generated")).unwrap();
        let snapshot = f.0.join("generated/MANIFEST.JSON");
        std::fs::write(&snapshot, &source).unwrap();
        data["entries"][0]["source"]["snapshot"] = json!("generated/MANIFEST.JSON");
        std::fs::write(&catalog, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(generate(&catalog, false)
            .unwrap_err()
            .contains("manifest collides with authored input"));
        assert_eq!(std::fs::read(snapshot).unwrap(), source);
        assert!(!f.0.join("generated/first.DWG").exists());
        assert!(!f.0.join("generated/second.DWG").exists());
    }
}
