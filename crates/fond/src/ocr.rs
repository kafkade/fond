use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use uuid::Uuid;

pub fn extract_text(image_path: &Path) -> Result<String> {
    let binary = std::env::var("FOND_TESSERACT_BIN").unwrap_or_else(|_| "tesseract".to_string());
    let output_base = temp_output_base();

    let output = Command::new(&binary)
        .arg(image_path)
        .arg(&output_base)
        .args(["--psm", "6"])
        .output()
        .with_context(|| format!("failed to run OCR backend '{binary}'"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            format!("OCR backend '{binary}' exited with {}", output.status)
        } else {
            format!("OCR backend '{binary}' failed: {stderr}")
        };
        cleanup_text_output(&output_base);
        bail!("{message}");
    }

    let text_path = output_base.with_extension("txt");
    let text = std::fs::read_to_string(&text_path).with_context(|| {
        format!(
            "OCR backend did not produce a text file at {}",
            text_path.display()
        )
    })?;
    cleanup_text_output(&output_base);

    if text.trim().is_empty() {
        return Err(anyhow!("OCR backend returned no text"));
    }

    Ok(text)
}

fn temp_output_base() -> PathBuf {
    std::env::temp_dir().join(format!(
        "fond-ocr-{}-{}",
        std::process::id(),
        Uuid::now_v7()
    ))
}

fn cleanup_text_output(output_base: &Path) {
    let _ = std::fs::remove_file(output_base.with_extension("txt"));
}
