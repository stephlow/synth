//! Focus caret for list / center cells.

pub fn focus_mark(focused: bool) -> &'static str {
    if focused {
        "›"
    } else {
        " "
    }
}
