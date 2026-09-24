use crate::common::{FormDataRow, FormDataType};
use std::path::Path;

/// Hand-rolled multipart/form-data encoder
pub(crate) async fn encode(
    form_data: Vec<FormDataRow>,
) -> Result<Option<(String, Vec<u8>)>, String> {
    let active_rows: Vec<FormDataRow> = form_data
        .into_iter()
        .filter(|row| row.is_active && !row.key.trim().is_empty())
        .collect();

    if active_rows.is_empty() {
        return Ok(None);
    }

    let boundary = format!("----RustRestBoundary{}", random_boundary_suffix());
    let mut body = Vec::new();

    for row in active_rows {
        match row.field_type {
            FormDataType::Text => {
                body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{}\"\r\n", row.key).as_bytes(),
                );
                let content_type = row.content_type.trim();
                if !content_type.is_empty() {
                    body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
                }
                body.extend_from_slice(b"\r\n");
                body.extend_from_slice(row.value.as_bytes());
                body.extend_from_slice(b"\r\n");
            }
            FormDataType::File => {
                // one part per file, all sharing the row's field name
                for file in &row.files {
                    if file.trim().is_empty() {
                        continue;
                    }
                    let path = Path::new(file);
                    if !path.exists() {
                        continue;
                    }
                    let file_bytes = tokio::fs::read(path)
                        .await
                        .map_err(|e| format!("Form File Read Failure: {}", e))?;
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("file")
                        .to_string();

                    let content_type = match row.content_type.trim() {
                        "" => guess_mime(&file_name),
                        explicit => explicit,
                    };
                    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
                    body.extend_from_slice(
                        format!(
                            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n\
                             Content-Type: {}\r\n\r\n",
                            row.key, file_name, content_type
                        )
                        .as_bytes(),
                    );
                    body.extend_from_slice(&file_bytes);
                    body.extend_from_slice(b"\r\n");
                }
            }
        }
    }

    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

    Ok(Some((boundary, body)))
}

/// a file part's `Content-Type` from its extension, for when the user didn't
/// set one; unknown extensions are sent as opaque bytes.
pub fn guess_mime(file_name: &str) -> &'static str {
    let ext = file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "json" => "application/json",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "txt" | "log" => "text/plain",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "text/javascript",
        "md" => "text/markdown",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// A random-enough boundary token; collision with request content is
/// astronomically unlikely and this avoids pulling in a `rand` dependency.
fn random_boundary_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}", nanos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn json_text_part_and_file_part_carry_their_content_types() {
        let path = std::env::temp_dir().join("rustrest-multipart-test.png");
        std::fs::write(&path, b"PNGDATA").unwrap();

        let mut json = FormDataRow::new("data", r#"{"a":1}"#, FormDataType::Text);
        json.content_type = "application/json".to_string();
        let plain = FormDataRow::new("note", "hi", FormDataType::Text);
        let mut file = FormDataRow::new("avatar", "", FormDataType::File);
        file.files = vec![path.to_str().unwrap().to_string()];

        let (_, body) = encode(vec![json, plain, file]).await.unwrap().unwrap();
        let body = String::from_utf8(body).unwrap();
        std::fs::remove_file(&path).ok();

        assert!(
            body.contains("name=\"data\"\r\nContent-Type: application/json\r\n\r\n{\"a\":1}\r\n")
        );
        assert!(body.contains("name=\"note\"\r\n\r\nhi\r\n"));
        assert!(body.contains(
            "filename=\"rustrest-multipart-test.png\"\r\nContent-Type: image/png\r\n\r\nPNGDATA\r\n"
        ));
    }

    #[tokio::test]
    async fn file_row_with_several_files_sends_one_part_each_under_the_same_key() {
        let dir = std::env::temp_dir();
        let a = dir.join("rustrest-multipart-multi-a.txt");
        let b = dir.join("rustrest-multipart-multi-b.json");
        std::fs::write(&a, b"AAA").unwrap();
        std::fs::write(&b, b"{}").unwrap();

        let mut row = FormDataRow::new("docs", "", FormDataType::File);
        row.files = vec![
            a.to_str().unwrap().to_string(),
            b.to_str().unwrap().to_string(),
        ];

        let (boundary, body) = encode(vec![row]).await.unwrap().unwrap();
        let body = String::from_utf8(body).unwrap();
        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();

        assert_eq!(body.matches(&format!("--{boundary}\r\n")).count(), 2);
        assert!(body.contains(
            "name=\"docs\"; filename=\"rustrest-multipart-multi-a.txt\"\r\nContent-Type: text/plain\r\n\r\nAAA\r\n"
        ));
        assert!(body.contains(
            "name=\"docs\"; filename=\"rustrest-multipart-multi-b.json\"\r\nContent-Type: application/json\r\n\r\n{}\r\n"
        ));
        assert!(body.ends_with(&format!("--{boundary}--\r\n")));
    }
}
