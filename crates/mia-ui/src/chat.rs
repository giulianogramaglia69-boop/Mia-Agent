//! Chat messages shown in the middle column.
//!
//! A message is one row of the conversation log. `ChatRow` wraps it with a
//! stable id, which the reactive `List` uses as its key.

/// Who a message comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatRole {
    /// The user typed it.
    User,
    /// Mia answered it (via the runtime loop, or a routing answer).
    Mia,
    /// A wiki page opened from the tree.
    Wiki,
}

/// One message in the conversation log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMessage {
    /// Who sent it.
    pub role: ChatRole,
    /// Plain text body.
    pub text: String,
}

impl ChatMessage {
    /// A message typed by the user.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            text: text.into(),
        }
    }

    /// A message answered by Mia.
    pub fn mia(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Mia,
            text: text.into(),
        }
    }

    /// A wiki page opened from the tree.
    pub fn wiki(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Wiki,
            text: text.into(),
        }
    }
}

/// A chat message with a stable list key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatRow {
    /// Monotonic id, assigned when the row is created.
    pub id: u64,
    /// The message itself.
    pub message: ChatMessage,
}

impl ChatRow {
    /// Wrap a message with its list key.
    pub fn new(id: u64, message: ChatMessage) -> Self {
        Self { id, message }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_keep_their_text() {
        let user = ChatMessage::user("ciao");
        let mia = ChatMessage::mia("ciao a te");
        let wiki = ChatMessage::wiki("# Titolo");
        assert_eq!(user.role, ChatRole::User);
        assert_eq!(mia.role, ChatRole::Mia);
        assert_eq!(wiki.role, ChatRole::Wiki);
        assert_eq!(user.text, "ciao");
        assert_eq!(ChatRow::new(7, mia).id, 7);
    }
}
