use tokio::io::{AsyncBufReadExt, BufReader};
use zuruf_speech::{Continuous, SpeechAdapter, Voxtype};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let v = Voxtype {
        executable: "voxtype".into(),
        model: std::env::var("ZURUF_MODEL").ok(),
        language: std::env::var("ZURUF_LANGUAGE").ok(),
        engine: Some("whisper".into()),
        directory: std::env::temp_dir().join("zuruf-dictate"),
        keep_audio: false,
    };
    let mut recorder = Continuous::new(v, 1);
    recorder.start("dictate".into()).await?;
    println!("Aufnahme läuft. Enter: Segment; Strg+D: beenden.");
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut tasks = Vec::new();
    while lines.next_line().await?.is_some() {
        let h = recorder.mark(None, None).await?;
        tasks.push(tokio::spawn(async move {
            println!("{:?}", h.resolve().await);
        }));
    }
    let final_segment = recorder.mark(None, None).await?;
    recorder.stop_recorder().await?;
    println!("{:?}", final_segment.resolve().await);
    for task in tasks {
        task.await?;
    }
    Ok(())
}
