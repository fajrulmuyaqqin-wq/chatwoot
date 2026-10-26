/// Splits a long document or web page into manageable chunks with overlap for embedding.
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    let clean_text = text.trim();
    if clean_text.is_empty() {
        return Vec::new();
    }

    if clean_text.len() <= chunk_size {
        return vec![clean_text.to_string()];
    }

    let mut chunks = Vec::new();
    let paragraphs: Vec<&str> = clean_text.split("\n\n").collect();

    let mut current_chunk = String::new();

    for para in paragraphs {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }

        if current_chunk.is_empty() {
            current_chunk.push_str(para);
        } else if current_chunk.len() + para.len() + 2 <= chunk_size {
            current_chunk.push_str("\n\n");
            current_chunk.push_str(para);
        } else {
            // Push accumulated chunk
            chunks.push(current_chunk.clone());

            // Prepare next chunk with overlap from the tail of current_chunk
            let tail_start = if current_chunk.len() > overlap {
                current_chunk.len() - overlap
            } else {
                0
            };
            let overlap_str = &current_chunk[tail_start..];

            current_chunk = format!("...{}\n\n{}", overlap_str.trim(), para);

            // If a single paragraph is larger than chunk_size, slice it
            while current_chunk.len() > chunk_size * 2 {
                let slice = &current_chunk[..chunk_size];
                chunks.push(slice.to_string());
                current_chunk = current_chunk[chunk_size..].to_string();
            }
        }
    }

    if !current_chunk.trim().is_empty() {
        chunks.push(current_chunk.trim().to_string());
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text_short() {
        let text = "Hello world";
        let chunks = chunk_text(text, 500, 50);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello world");
    }

    #[test]
    fn test_chunk_text_paragraphs() {
        let text = "Para 1.\n\nPara 2.\n\nPara 3.";
        let chunks = chunk_text(text, 15, 5);
        assert!(chunks.len() >= 2);
    }
}
