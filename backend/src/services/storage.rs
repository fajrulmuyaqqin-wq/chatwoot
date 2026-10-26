use std::path::Path;
use tokio::fs;
use uuid::Uuid;

#[allow(dead_code)]
pub struct StoredFile {
    pub file_name: String,
    pub relative_url: String,
    pub extension: String,
    pub size_bytes: usize,
}

pub async fn save_upload(
    bytes: &[u8],
    original_filename: Option<&str>,
) -> Result<StoredFile, std::io::Error> {
    let upload_dir = "./storage/uploads";
    if !Path::new(upload_dir).exists() {
        fs::create_dir_all(upload_dir).await?;
    }

    let ext = original_filename
        .and_then(|f| Path::new(f).extension().and_then(|e| e.to_str()))
        .unwrap_or("bin")
        .to_lowercase();

    let unique_id = Uuid::new_v4().to_string();
    let file_name = format!("{}.{}", unique_id, ext);
    let file_path = format!("{}/{}", upload_dir, file_name);

    fs::write(&file_path, bytes).await?;

    Ok(StoredFile {
        file_name: file_name.clone(),
        relative_url: format!("/storage/uploads/{}", file_name),
        extension: ext,
        size_bytes: bytes.len(),
    })
}
