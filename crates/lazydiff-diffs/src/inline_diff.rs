#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineDiffSpan {
    pub start: usize,
    pub end: usize,
}

const MAX_INLINE_DIFF_LINE_BYTES: usize = 4_096;
const MAX_INLINE_DIFF_TOKENS: usize = 512;

/// Pierre-inspired `word-alt` inline diff primitive.
///
/// It tokenizes two paired changed lines, finds unchanged tokens with LCS, and
/// returns changed byte ranges for each side. Like Pierre, it joins changed
/// runs across a single whitespace gap so tiny neutral spaces do not leave
/// distracting holes in the highlight.
pub fn compute_inline_diff_spans(
    delete_text: &str,
    add_text: &str,
) -> Option<(Vec<InlineDiffSpan>, Vec<InlineDiffSpan>)> {
    if delete_text == add_text
        || delete_text.len() > MAX_INLINE_DIFF_LINE_BYTES
        || add_text.len() > MAX_INLINE_DIFF_LINE_BYTES
    {
        return None;
    }

    let delete_tokens = diff_tokens(delete_text);
    let add_tokens = diff_tokens(add_text);
    if delete_tokens.is_empty()
        || add_tokens.is_empty()
        || delete_tokens.len() > MAX_INLINE_DIFF_TOKENS
        || add_tokens.len() > MAX_INLINE_DIFF_TOKENS
    {
        return None;
    }

    let common = lcs_pairs(&delete_tokens, &add_tokens);
    let mut delete_changed = vec![true; delete_tokens.len()];
    let mut add_changed = vec![true; add_tokens.len()];
    for (delete_index, add_index) in common {
        delete_changed[delete_index] = false;
        add_changed[add_index] = false;
    }

    Some((
        changed_token_spans(delete_text, &delete_tokens, &delete_changed),
        changed_token_spans(add_text, &add_tokens, &add_changed),
    ))
}

#[derive(Clone, Copy)]
struct DiffToken<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn diff_tokens(text: &str) -> Vec<DiffToken<'_>> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut current_kind: Option<TokenKind> = None;

    for (byte_index, ch) in text.char_indices() {
        let kind = TokenKind::for_char(ch);
        if let Some(current_kind) = current_kind {
            if current_kind != kind || kind == TokenKind::Punctuation {
                tokens.push(DiffToken {
                    text: &text[start..byte_index],
                    start,
                    end: byte_index,
                });
                start = byte_index;
            }
        }
        current_kind = Some(kind);
    }

    if start < text.len() {
        tokens.push(DiffToken {
            text: &text[start..],
            start,
            end: text.len(),
        });
    }

    tokens
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Word,
    Whitespace,
    Punctuation,
}

impl TokenKind {
    fn for_char(ch: char) -> Self {
        if ch.is_whitespace() {
            Self::Whitespace
        } else if ch.is_alphanumeric() || ch == '_' {
            Self::Word
        } else {
            Self::Punctuation
        }
    }
}

fn lcs_pairs(left: &[DiffToken<'_>], right: &[DiffToken<'_>]) -> Vec<(usize, usize)> {
    let width = right.len() + 1;
    let mut table = vec![0u16; (left.len() + 1) * width];

    for i in (0..left.len()).rev() {
        for j in (0..right.len()).rev() {
            table[i * width + j] = if left[i].text == right[j].text {
                table[(i + 1) * width + j + 1].saturating_add(1)
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }

    let mut pairs = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        if left[i].text == right[j].text {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if table[(i + 1) * width + j] >= table[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }

    pairs
}

fn changed_token_spans(
    text: &str,
    tokens: &[DiffToken<'_>],
    changed: &[bool],
) -> Vec<InlineDiffSpan> {
    let mut spans: Vec<InlineDiffSpan> = Vec::new();
    for (token, changed) in tokens.iter().zip(changed) {
        if !*changed || token.start >= token.end {
            continue;
        }
        if let Some(last) = spans.last_mut() {
            let gap = &text[last.end..token.start];
            if gap.is_empty() || (gap.len() == 1 && gap.chars().all(char::is_whitespace)) {
                last.end = token.end;
                continue;
            }
        }
        spans.push(InlineDiffSpan {
            start: token.start,
            end: token.end,
        });
    }
    spans
}
