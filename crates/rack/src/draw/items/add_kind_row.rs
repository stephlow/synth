//! Add-module line.

pub struct AddKindRowView<'a> {
    pub mark: &'a str,
    pub display_name: &'a str,
    pub kind: &'a str,
}

pub fn format_add_kind_row(v: AddKindRowView<'_>) -> String {
    format!("{}{}   {}", v.mark, v.display_name, v.kind)
}
