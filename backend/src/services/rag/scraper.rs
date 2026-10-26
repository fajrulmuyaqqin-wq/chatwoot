use tracing::info;

/// Fetches web page content from a URL and extracts title and clean readable text
pub async fn fetch_url_content(url: &str) -> Result<(String, String), String> {
    info!("Fetching URL content for RAG pipeline: {}", url);
    let client = match reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(20))
        .build()
    {
        Ok(c) => c,
        Err(e) => return Err(format!("Failed to build HTTP client: {e}")),
    };

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch URL: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("URL returned HTTP error status: {}", status));
    }

    let html = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    let title = extract_title(&html).unwrap_or_else(|| url.to_string());
    let clean_text = clean_html_to_text(&html);

    if clean_text.trim().is_empty() {
        return Ok((title, format!("Dokumen dari {}. Konten tidak memuat teks statis.", url)));
    }

    Ok((title, clean_text))
}

/// Simple, robust HTML cleaner that strips script, style, and HTML tags
pub fn clean_html_to_text(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;

    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();
    let len = chars.len();

    let mut i = 0;
    while i < len {
        if !in_tag && chars[i] == '<' {
            in_tag = true;

            // Check for <script or <style
            if i + 7 < len && &lower_chars[i..i + 7] == ['<', 's', 'c', 'r', 'i', 'p', 't'] {
                in_script = true;
            } else if i + 6 < len && &lower_chars[i..i + 6] == ['<', 's', 't', 'y', 'l', 'e'] {
                in_style = true;
            } else if in_script && i + 9 <= len && &lower_chars[i..i + 9] == ['<', '/', 's', 'c', 'r', 'i', 'p', 't', '>'] {
                in_script = false;
                in_tag = false;
                i += 9;
                continue;
            } else if in_style && i + 8 <= len && &lower_chars[i..i + 8] == ['<', '/', 's', 't', 'y', 'l', 'e', '>'] {
                in_style = false;
                in_tag = false;
                i += 8;
                continue;
            }

            i += 1;
            continue;
        }

        if in_tag {
            if chars[i] == '>' {
                in_tag = false;
                result.push(' ');
            }
            i += 1;
            continue;
        }

        if !in_script && !in_style {
            result.push(chars[i]);
        }

        i += 1;
    }

    // Decode common entities and normalize whitespace
    let decoded = result
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");

    let mut normalized = String::new();
    let mut prev_whitespace = false;

    for ch in decoded.chars() {
        if ch.is_whitespace() {
            if !prev_whitespace {
                normalized.push(' ');
                prev_whitespace = true;
            }
        } else {
            normalized.push(ch);
            prev_whitespace = false;
        }
    }

    normalized.trim().to_string()
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start_idx = lower.find("<title>")?;
    let end_idx = lower.find("</title>")?;
    if end_idx > start_idx + 7 {
        let title = &html[start_idx + 7..end_idx];
        Some(clean_html_to_text(title))
    } else {
        None
    }
}

/// Extracts printable text content from PDF byte streams
pub fn extract_pdf_text(bytes: &[u8]) -> String {
    // Scan PDF for text blocks (BT ... ET) and printable ASCII strings
    let mut extracted = String::new();
    let mut i = 0;
    let len = bytes.len();

    let mut in_bt = false;
    let mut in_paren = false;
    let mut current_word = Vec::new();

    while i < len {
        if !in_bt && i + 2 <= len && &bytes[i..i + 2] == b"BT" {
            in_bt = true;
            i += 2;
            continue;
        }

        if in_bt && i + 2 <= len && &bytes[i..i + 2] == b"ET" {
            in_bt = false;
            extracted.push('\n');
            i += 2;
            continue;
        }

        if in_bt {
            if bytes[i] == b'(' && (i == 0 || bytes[i - 1] != b'\\') {
                in_paren = true;
                current_word.clear();
            } else if bytes[i] == b')' && (i == 0 || bytes[i - 1] != b'\\') {
                in_paren = false;
                if let Ok(s) = String::from_utf8(current_word.clone()) {
                    extracted.push_str(&s);
                    extracted.push(' ');
                }
            } else if in_paren {
                current_word.push(bytes[i]);
            }
        }

        i += 1;
    }

    if extracted.trim().len() > 50 {
        extracted.trim().to_string()
    } else {
        // Fallback: extract contiguous ASCII/UTF-8 readable chunks
        let mut fallback = String::new();
        let mut temp = Vec::new();
        for &b in bytes {
            if (b >= 32 && b <= 126) || b == b'\n' || b == b'\t' {
                temp.push(b);
            } else {
                if temp.len() >= 4 {
                    if let Ok(s) = std::str::from_utf8(&temp) {
                        fallback.push_str(s);
                        fallback.push(' ');
                    }
                }
                temp.clear();
            }
        }
        if temp.len() >= 4 {
            if let Ok(s) = std::str::from_utf8(&temp) {
                fallback.push_str(s);
            }
        }
        fallback.trim().to_string()
    }
}
