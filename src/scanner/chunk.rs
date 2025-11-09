#[derive(Debug, Clone)]
pub struct TextChunk {
    pub text: String,
    pub offset: usize,
}

pub fn chunk_text(text: &str, max_chunk_size: usize) -> Vec<TextChunk> {
    if text.len() <= max_chunk_size {
        return vec![TextChunk {
            text: text.to_string(),
            offset: 0,
        }];
    }

    let mut chunks = Vec::new();
    let mut start = 0usize;
    let total_len = text.len();

    while start < total_len {
        let mut end = (start + max_chunk_size).min(total_len);

        if end < total_len {
            let slice = &text[start..end];
            if let Some(pos) = slice.rfind('\n') {
                if pos > 0 {
                    end = start + pos + 1;
                }
            } else if let Some(pos) = slice.rfind(' ') {
                if pos > 0 {
                    end = start + pos + 1;
                }
            }
        }

        let chunk = &text[start..end];
        chunks.push(TextChunk {
            text: chunk.to_string(),
            offset: start,
        });
        start = end;

        // Guard against slicing mid-code-point.
        while start < total_len && !text.is_char_boundary(start) {
            start += 1;
        }
    }

    chunks
}
