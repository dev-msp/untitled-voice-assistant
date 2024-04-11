pub struct Iter(String);

impl From<String> for Iter {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Iter {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl Iter {
    pub fn words(&self) -> impl Iterator<Item = (usize, String)> + Clone + '_ {
        let max_char_idx = self.0.chars().count() - 1;
        self.0
            .chars()
            // Accumulate words and their offsets in bytes
            .enumerate()
            .scan(
                (0, 0, String::new()),
                move |(total_offset, word_start_offset, word), (i, c)| {
                    let bs = c.len_utf8();

                    if !c.is_whitespace() {
                        // Mark the start of the word.
                        if word.is_empty() && total_offset != word_start_offset {
                            *word_start_offset = *total_offset;
                        }

                        word.push(c);
                    }

                    // Advance the offset.
                    *total_offset += bs;

                    // When we reach a word boundary and the word is at least one character long, or we
                    // know this is the last character, then output the word along with its start
                    // offset.
                    if (c.is_whitespace() && !word.is_empty()) || i == max_char_idx {
                        let w = std::mem::take(word);
                        Some(Some((*word_start_offset, w)))
                    } else {
                        Some(None)
                    }
                    // I need to flush what's left at the end somehow.
                },
            )
            .flatten()
    }
}
