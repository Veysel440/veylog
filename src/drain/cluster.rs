//! A cluster: the messages that match one pattern, and how the pattern is stored.

use std::rc::Rc;

use super::{WILDCARD, WILDCARD_DISPLAY};

/// Messages that match the same pattern.
///
/// A pattern only turns into wildcards (`update`): its tokens that are not wildcards are always
/// those of the message that opened the cluster. So the tokens are not stored separately; the
/// pattern is the text of the opening message (shared with the masked message in the store) and
/// the places of the tokens in that text. Eight bytes per token: in a log full of long SQL
/// messages, the patterns take a seventh of the memory they would take if every token were a
/// separate string.
#[derive(Debug, Clone)]
pub struct Cluster {
    /// The message that opened the cluster.
    pub(super) origin: Rc<str>,
    /// The places of the pattern's tokens in `origin` (start and end, in bytes); `WILDCARD_SPAN`
    /// for a wildcard. How it is shown: `text`.
    spans: Box<[(u32, u32)]>,
    /// The number of messages that joined the cluster.
    pub count: usize,
}

/// The place of a wildcard in a pattern: a range that is in no text.
pub(super) const WILDCARD_SPAN: (u32, u32) = (u32::MAX, u32::MAX);

impl Cluster {
    /// The cluster opened by the message `origin`; `bounds`: the places of its tokens
    /// (`token_bounds`).
    pub(super) fn new(origin: &Rc<str>, bounds: &[(usize, usize)]) -> Cluster {
        let offset = |pos: usize| u32::try_from(pos).expect("a message is shorter than 4 GB");
        Cluster {
            origin: Rc::clone(origin),
            spans: bounds
                .iter()
                .map(|&(start, end)| (offset(start), offset(end)))
                .collect(),
            count: 1,
        }
    }

    /// The tokens of the pattern in order; the internal wildcard marker (`WILDCARD`) for the
    /// wildcards.
    pub(super) fn tokens(&self) -> impl Iterator<Item = &str> + '_ {
        self.spans.iter().map(|&(start, end)| {
            if (start, end) == WILDCARD_SPAN {
                WILDCARD
            } else {
                &self.origin[start as usize..end as usize]
            }
        })
    }

    /// Turns the positions where the pattern differs from the message into wildcards. True if the
    /// pattern changed.
    pub(super) fn update(&mut self, tokens: &[&str]) -> bool {
        let mut changed = false;
        for (span, t) in self.spans.iter_mut().zip(tokens) {
            if *span != WILDCARD_SPAN && &self.origin[span.0 as usize..span.1 as usize] != *t {
                *span = WILDCARD_SPAN;
                changed = true;
            }
        }
        changed
    }

    /// The indexes (from 0) of the wildcard positions in the pattern. A `<*>` written in the
    /// message is not a wildcard.
    pub fn wildcard_positions(&self) -> impl Iterator<Item = usize> + '_ {
        self.spans
            .iter()
            .enumerate()
            .filter(|&(_, &span)| span == WILDCARD_SPAN)
            .map(|(i, _)| i)
    }

    /// The readable form of the pattern: "Scheduled task started: <*>"
    pub fn text(&self) -> String {
        let tokens: Vec<&str> = self
            .tokens()
            .map(|token| {
                if token == WILDCARD {
                    WILDCARD_DISPLAY
                } else {
                    token
                }
            })
            .collect();
        tokens.join(" ")
    }
}

/// The places of a message's tokens (start and end, in bytes): split at Unicode whitespace, like
/// `split_whitespace`.
pub(super) fn token_bounds(message: &str) -> Vec<(usize, usize)> {
    let mut bounds = Vec::new();
    let mut start = None;
    for (pos, c) in message.char_indices() {
        if c.is_whitespace() {
            if let Some(start) = start.take() {
                bounds.push((start, pos));
            }
        } else if start.is_none() {
            start = Some(pos);
        }
    }
    if let Some(start) = start {
        bounds.push((start, message.len()));
    }
    bounds
}
