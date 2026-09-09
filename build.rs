// Keep templates/ in sync with ../unitframes/templates (the output of
// build_unitframes.py) when that directory exists, so `cargo build` picks up a
// regenerated template automatically.  A standalone checkout of this crate
// just uses the copies committed under templates/.
use std::{fs, path::Path};

fn copy_dir(src: &Path, dst: &Path) {
    if let Ok(rd) = fs::read_dir(src) {
        for e in rd.flatten() {
            let p = e.path();
            let d = dst.join(e.file_name());
            if p.is_dir() {
                let _ = fs::create_dir_all(&d);
                copy_dir(&p, &d);
            } else if p.extension().and_then(|s| s.to_str()) == Some("inc") {
                let _ = fs::copy(&p, &d);
            }
        }
    }
}

fn main() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../unitframes/templates");
    println!("cargo:rerun-if-changed=../unitframes/templates");
    println!("cargo:rerun-if-changed=templates");
    if src.is_dir() {
        copy_dir(&src, &Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"));
    }
}
