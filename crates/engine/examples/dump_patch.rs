
use synth_proto::synth_client::SynthClient;
use synth_proto::Empty;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut c = SynthClient::connect("http://127.0.0.1:50051").await?;
    let snap = c.get_patch(Empty {}).await?.into_inner();
    std::fs::write("/tmp/live_patch.json", &snap.document_json)?;
    println!("bytes {}", snap.document_json.len());
    Ok(())
}
