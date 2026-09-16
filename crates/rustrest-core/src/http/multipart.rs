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
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());

        match row.field_type {
            FormDataType::Text => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                        row.key
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(row.value.as_bytes());
                body.extend_from_slice(b"\r\n");
            }
            FormDataType::File => {
                if row.value.trim().is_empty() {
                    continue;
                }
                let path = Path::new(&row.value);
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

                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n\r\n",
                        row.key, file_name
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(&file_bytes);
                body.extend_from_slice(b"\r\n");
            }
        }
    }

    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

    Ok(Some((boundary, body)))
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
