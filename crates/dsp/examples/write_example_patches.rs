//! Write repo example patches into `patches/` (relative to CWD).
//!
//! ```sh
//! cargo run -p dsp --example write_example_patches
//! ```

use dsp::Patch;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let dir = root.join("patches");
    std::fs::create_dir_all(&dir)?;

    let examples = [
        ("empty.json", Patch::empty()),
        ("groove.json", Patch::example_groove()),
    ];
    for (name, patch) in examples {
        let path = dir.join(name);
        let json = patch.to_json()?;
        std::fs::write(&path, json)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
