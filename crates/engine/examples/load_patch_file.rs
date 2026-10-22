//! One-shot: load a patch JSON file into a running engine via gRPC.
use synth_proto::synth_client::SynthClient;
use synth_proto::LoadPatchRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("usage: load_patch_file <patch.json>");
    let addr = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "http://127.0.0.1:50051".into());
    let document_json = std::fs::read_to_string(&path)?;
    let mut client = SynthClient::connect(addr).await?;
    let snap = client
        .load_patch(LoadPatchRequest { document_json })
        .await?
        .into_inner();
    println!(
        "loaded modules={} cables={}",
        snap.modules.len(),
        snap.cables.len()
    );
    for m in &snap.modules {
        let c = m
            .coordinates
            .as_ref()
            .map(|c| format!("({},{})", c.x, c.y))
            .unwrap_or_else(|| "(0,0)".into());
        println!("  {} {} {}", m.id, m.name, c);
    }
    Ok(())
}
