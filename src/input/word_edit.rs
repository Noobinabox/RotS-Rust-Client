use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy)]
pub(super) enum WordEdit {
    Left,
    Right,
    DeleteLeft,
    DeleteRight,
}

impl WordEdit {
    pub(super) fn from_key(key: KeyEvent) -> Option<Self> {
        match (key.code, key.modifiers) {
            (KeyCode::Left, KeyModifiers::CONTROL) | (KeyCode::Char('b'), KeyModifiers::ALT) => {
                Some(Self::Left)
            }
            (KeyCode::Right, KeyModifiers::CONTROL) | (KeyCode::Char('f'), KeyModifiers::ALT) => {
                Some(Self::Right)
            }
            (KeyCode::Backspace, KeyModifiers::CONTROL | KeyModifiers::ALT)
            | (KeyCode::Char('w'), KeyModifiers::CONTROL) => Some(Self::DeleteLeft),
            (KeyCode::Delete, KeyModifiers::CONTROL) | (KeyCode::Char('d'), KeyModifiers::ALT) => {
                Some(Self::DeleteRight)
            }
            _ => None,
        }
    }

    pub(super) fn deletes(self) -> bool {
        matches!(self, Self::DeleteLeft | Self::DeleteRight)
    }

    pub(super) fn apply(self, text: &mut String, cursor: &mut usize) {
        let backward = matches!(self, Self::Left | Self::DeleteLeft);
        let boundary = if backward {
            let prefix = &text[..*cursor];
            let trimmed = prefix.trim_end_matches(char::is_whitespace);
            trimmed
                .char_indices()
                .rev()
                .find(|(_, ch)| ch.is_whitespace())
                .map_or(0, |(index, ch)| index + ch.len_utf8())
        } else {
            let suffix = &text[*cursor..];
            let trimmed = suffix.trim_start_matches(char::is_whitespace);
            text.len() - trimmed.len() + trimmed.find(char::is_whitespace).unwrap_or(trimmed.len())
        };
        if self.deletes() {
            text.replace_range((*cursor).min(boundary)..(*cursor).max(boundary), "");
        }
        if backward || !self.deletes() {
            *cursor = boundary;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_ranges_cover_whitespace_punctuation_and_unicode() {
        for (input, cursor, left, right) in [
            ("", 0, 0, 0),
            ("  \n", 1, 0, 3),
            ("say hello-world next", 9, 4, 15),
            ("say hello", 3, 0, 9),
            ("say hello", 4, 0, 9),
            ("é\u{2003}猫\nnext", 8, 5, 13),
            ("e\u{301} 👩‍💻", 3, 0, 15),
        ] {
            for (edit, expected) in [(WordEdit::Left, left), (WordEdit::Right, right)] {
                let mut text = input.to_owned();
                let mut position = cursor;
                edit.apply(&mut text, &mut position);
                assert_eq!(position, expected, "{input:?}");
                assert_eq!(text, input);
            }
        }
    }

    #[test]
    fn deletion_and_movement_are_safe_at_every_character_boundary() {
        let original = "  é\u{2003}猫\nhello-world e\u{301} 👩‍💻  ";
        for cursor in original
            .char_indices()
            .map(|(i, _)| i)
            .chain([original.len()])
        {
            for (movement, deletion) in [
                (WordEdit::Left, WordEdit::DeleteLeft),
                (WordEdit::Right, WordEdit::DeleteRight),
            ] {
                let mut text = original.to_owned();
                let mut boundary = cursor;
                movement.apply(&mut text, &mut boundary);
                assert!(text.is_char_boundary(boundary));
                let expected = format!(
                    "{}{}",
                    &original[..cursor.min(boundary)],
                    &original[cursor.max(boundary)..]
                );
                let mut position = cursor;
                deletion.apply(&mut text, &mut position);
                assert_eq!(text, expected);
                assert_eq!(position, cursor.min(boundary));
                assert!(text.is_char_boundary(position));
            }
        }
    }

    #[test]
    fn shortcuts_use_exact_modifiers() {
        for (code, modifiers) in [
            (KeyCode::Left, KeyModifiers::CONTROL),
            (KeyCode::Right, KeyModifiers::CONTROL),
            (KeyCode::Backspace, KeyModifiers::CONTROL),
            (KeyCode::Backspace, KeyModifiers::ALT),
            (KeyCode::Delete, KeyModifiers::CONTROL),
            (KeyCode::Char('w'), KeyModifiers::CONTROL),
            (KeyCode::Char('b'), KeyModifiers::ALT),
            (KeyCode::Char('f'), KeyModifiers::ALT),
            (KeyCode::Char('d'), KeyModifiers::ALT),
        ] {
            assert!(WordEdit::from_key(KeyEvent::new(code, modifiers)).is_some());
            assert!(
                WordEdit::from_key(KeyEvent::new(code, modifiers | KeyModifiers::SHIFT)).is_none()
            );
        }
    }
}
