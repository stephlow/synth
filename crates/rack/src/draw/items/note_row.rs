//! Keyboard note line.

pub struct NoteRowView<'a> {
    pub mark: &'a str,
    pub label: char,
    pub midi: u8,
    pub held: bool,
}

pub fn format_note_row(v: NoteRowView<'_>) -> String {
    let state = if v.held { "on " } else { "   " };
    format!("{} {}  {}  {}", v.mark, v.label, state, v.midi)
}
