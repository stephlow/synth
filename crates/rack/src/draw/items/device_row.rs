//! IO device line.

use crate::draw::text::truncate;

pub struct DeviceRowView<'a> {
    pub mark: &'a str,
    pub device_name: &'a str,
    pub max_width: usize,
}

pub fn format_device_row(v: DeviceRowView<'_>) -> String {
    let short = truncate(v.device_name, v.max_width.saturating_sub(10));
    format!("{}device  {}", v.mark, short)
}
