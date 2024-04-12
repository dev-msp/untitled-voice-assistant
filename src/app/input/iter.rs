use std::ops::{Deref, DerefMut};

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

#[derive(Debug, Clone, Default)]
pub struct ByteOffsetString {
    segment_start: usize,
    cursor: usize,
    sub: String,
}

impl AsRef<str> for ByteOffsetString {
    fn as_ref(&self) -> &str {
        &self.sub
    }
}

impl Deref for ByteOffsetString {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.sub
    }
}

impl DerefMut for ByteOffsetString {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.sub
    }
}

impl std::fmt::Display for ByteOffsetString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.sub)
    }
}
impl ByteOffsetString {
    pub fn segment_offset(&self) -> usize {
        self.segment_start
    }

    fn consume(&mut self, c: char) {
        self.sub.push(c);
        self.cursor += c.len_utf8();
    }

    fn swap(&mut self) -> Self {
        let new = Self {
            segment_start: self.cursor,
            cursor: self.cursor,
            sub: String::new(),
        };

        std::mem::replace(self, new)
    }
}

pub fn alpha_only(mut bos: ByteOffsetString) -> ByteOffsetString {
    let new_s = bos
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect::<String>();
    *bos = new_s;
    bos
}

impl Iter {
    pub fn words(&self) -> impl Iterator<Item = ByteOffsetString> + Clone + '_ {
        let max_char_idx = self.0.chars().count() - 1;
        self.0
            .chars()
            // Accumulate words and their offsets in bytes
            .enumerate()
            .scan(ByteOffsetString::default(), move |bos, (i, c)| {
                if !c.is_whitespace() {
                    bos.consume(c);
                }

                // When we reach a word boundary and the word is at least one character long, or we
                // know this is the last character, then output the word along with its start
                // offset.
                if (c.is_whitespace() && !bos.is_empty()) || i == max_char_idx {
                    let new_bos = bos.swap();
                    Some(Some(new_bos))
                } else {
                    Some(None)
                }
                // I need to flush what's left at the end somehow.
            })
            .flatten()
    }
}
