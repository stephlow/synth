fn main() {
    let json = std::fs::read_to_string("/tmp/morph_patch.json").unwrap();
    let patch = dsp::Patch::from_json(&json).expect("parse");
    let _ = patch.build(48_000.0).expect("build");
    println!("ok {} modules {} cables", patch.modules.len(), patch.cables.len());
}
