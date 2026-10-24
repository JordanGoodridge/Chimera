//! The shared model of a game's modal popups: a message box (OK), a
//! confirm box (OK, Cancel) and a text input (OK, Cancel; a password box
//! masks what is typed). A plain struct each game's screen holds and
//! unit-tests, like `widgets`' other state machines: `Dialog::handle`
//! takes what happened (`Press`) and says what the player decided
//! (`Outcome`); the game draws it in its own look (Combat Arms' lobby
//! from its XML layouts, CS:S's GameUI dialogs from `widgets`) and acts on
//! the outcome.
//!
//! The text entry is `widgets`' (`Caret`, `Edit`): the same caret,
//! selection and byte limit as every other entry.

use super::widgets::{Caret, Edit};

/// What a dialog asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Tells something; OK closes it.
    Message,
    /// Asks yes or no.
    Confirm,
    /// Asks for text.
    Input,
}

/// A dialog's buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Ok,
    Cancel,
}

/// What happened to the dialog.
#[derive(Clone, Debug, PartialEq)]
pub enum Press {
    /// Typed or pasted text.
    Type(String),
    Backspace,
    /// A caret or selection key in the entry.
    Edit(Edit),
    /// A click into the entry at a char.
    Caret(usize),
    Enter,
    Escape,
    Click(Button),
}

/// What the player decided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A message was read, or a confirm accepted.
    Ok,
    Cancel,
    /// An input's text, as typed.
    Submit(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dialog {
    pub kind: Kind,
    pub title: String,
    /// The message or the question.
    pub text: String,
    /// What is typed (an input).
    pub input: String,
    pub caret: Caret,
    /// The input's limit in bytes.
    pub max: usize,
    /// Show the input as stars.
    pub masked: bool,
}

impl Dialog {
    fn new(kind: Kind, title: &str, text: &str) -> Self {
        Self {
            kind,
            title: title.into(),
            text: text.into(),
            input: String::new(),
            caret: Caret::default(),
            max: 256,
            masked: false,
        }
    }

    pub fn message(title: &str, text: &str) -> Self {
        Self::new(Kind::Message, title, text)
    }

    pub fn confirm(title: &str, text: &str) -> Self {
        Self::new(Kind::Confirm, title, text)
    }

    pub fn input(title: &str, text: &str, max: usize) -> Self {
        Self {
            max,
            ..Self::new(Kind::Input, title, text)
        }
    }

    /// A password box: an input that shows stars.
    pub fn password(title: &str, text: &str, max: usize) -> Self {
        Self {
            masked: true,
            ..Self::input(title, text, max)
        }
    }

    /// The input as it is drawn.
    pub fn shown_input(&self) -> String {
        if self.masked {
            "*".repeat(self.input.chars().count())
        } else {
            self.input.clone()
        }
    }

    /// Forget what was typed (the box opens again empty).
    pub fn clear(&mut self) {
        self.input.clear();
        self.caret = Caret::default();
    }

    fn edit(&mut self, e: Edit) {
        if self.kind == Kind::Input {
            let mut caret = self.caret;
            caret.edit(&mut self.input, e, self.max);
            self.caret = caret;
        }
    }

    /// Apply `press`; `Some` when the player has decided and the dialog
    /// should close. Typing in a dialog with no entry does nothing.
    pub fn handle(&mut self, press: Press) -> Option<Outcome> {
        match press {
            Press::Type(s) => self.edit(Edit::Insert(s)),
            Press::Backspace => self.edit(Edit::Backspace),
            Press::Edit(e) => self.edit(e),
            Press::Caret(at) => self.caret = Caret { at, anchor: at },
            Press::Enter | Press::Click(Button::Ok) => return Some(self.accept()),
            Press::Escape | Press::Click(Button::Cancel) => {
                // A message has no Cancel: Esc reads it.
                return Some(if self.kind == Kind::Message { Outcome::Ok } else { Outcome::Cancel });
            }
        }
        None
    }

    fn accept(&self) -> Outcome {
        match self.kind {
            Kind::Input => Outcome::Submit(self.input.clone()),
            _ => Outcome::Ok,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_closes_on_ok_enter_or_escape() {
        for p in [Press::Click(Button::Ok), Press::Enter, Press::Escape] {
            assert_eq!(Dialog::message("t", "m").handle(p), Some(Outcome::Ok));
        }
        let mut d = Dialog::message("t", "m");
        assert_eq!(d.handle(Press::Type("x".into())), None);
        assert!(d.input.is_empty(), "no entry to type into");
    }

    #[test]
    fn a_confirm_says_yes_or_no() {
        assert_eq!(Dialog::confirm("t", "q").handle(Press::Enter), Some(Outcome::Ok));
        assert_eq!(Dialog::confirm("t", "q").handle(Press::Escape), Some(Outcome::Cancel));
        assert_eq!(Dialog::confirm("t", "q").handle(Press::Click(Button::Cancel)), Some(Outcome::Cancel));
    }

    #[test]
    fn an_input_takes_text_up_to_its_limit_and_submits_it() {
        let mut d = Dialog::input("t", "name?", 4);
        assert_eq!(d.handle(Press::Type("ab".into())), None);
        assert_eq!(d.handle(Press::Type("cdef".into())), None);
        assert_eq!(d.input, "abcd");
        d.handle(Press::Backspace);
        assert_eq!(d.handle(Press::Enter), Some(Outcome::Submit("abc".into())));
        assert_eq!(d.handle(Press::Escape), Some(Outcome::Cancel));
    }

    #[test]
    fn a_password_shows_stars_and_edits_at_the_caret() {
        let mut d = Dialog::password("t", "pw", 8);
        d.handle(Press::Type("abc".into()));
        assert_eq!(d.shown_input(), "***");
        d.handle(Press::Caret(1));
        d.handle(Press::Type("X".into()));
        assert_eq!(d.input, "aXbc");
        d.clear();
        assert_eq!((d.input.as_str(), d.caret), ("", Caret::default()));
    }
}
