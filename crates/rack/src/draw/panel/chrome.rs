//! Module chassis chrome — double-line when focused, title plate on the rim.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::Frame;

use crate::draw::style::{border_style, strong, title_style};

pub struct PanelChromeView<'a> {
    pub title: &'a str,
    pub kind: &'a str,
    pub focused: bool,
}

/// Draw chassis border + title plate; return inner content rect.
pub fn draw_chrome(frame: &mut Frame, area: Rect, view: PanelChromeView<'_>) -> Rect {
    if area.width < 8 || area.height < 4 {
        return area;
    }

    let style = border_style(view.focused);
    let buf = frame.buffer_mut();
    paint_chassis(buf, area, style, view.focused);

    // Title plate inset on top edge: ═◈ Title ═
    let plate = if view.focused {
        format!("◈ {} ", view.title)
    } else {
        format!(" {} ", view.title)
    };
    let max = area.width.saturating_sub(4) as usize;
    let plate = crate::draw::text::truncate(&plate, max);
    let start = area.x + 2;
    let tstyle = title_style(view.focused);
    for (i, ch) in plate.chars().enumerate() {
        let x = start + i as u16;
        if x >= area.x + area.width - 1 {
            break;
        }
        if let Some(cell) = buf.cell_mut((x, area.y)) {
            cell.set_char(ch);
            cell.set_style(tstyle);
        }
    }

    // Kind tag bottom edge (right)
    let tag = format!(" {} ", view.kind);
    let tag_chars: Vec<char> = tag.chars().collect();
    let tw = tag_chars.len() as u16;
    let tx0 = area.x + area.width.saturating_sub(tw + 1);
    let ty = area.y + area.height - 1;
    for (i, ch) in tag_chars.into_iter().enumerate() {
        let x = tx0 + i as u16;
        if x <= area.x || x >= area.x + area.width - 1 {
            continue;
        }
        if let Some(cell) = buf.cell_mut((x, ty)) {
            cell.set_char(ch);
            cell.set_style(if view.focused { strong() } else { style });
        }
    }

    Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

fn paint_chassis(buf: &mut Buffer, area: Rect, style: Style, focused: bool) {
    let x0 = area.x;
    let y0 = area.y;
    let x1 = area.x + area.width - 1;
    let y1 = area.y + area.height - 1;

    let (tl, tr, bl, br, h, v) = if focused {
        ('╔', '╗', '╚', '╝', '═', '║')
    } else {
        ('┌', '┐', '└', '┘', '─', '│')
    };

    set(buf, x0, y0, tl, style);
    set(buf, x1, y0, tr, style);
    set(buf, x0, y1, bl, style);
    set(buf, x1, y1, br, style);

    for x in (x0 + 1)..x1 {
        set(buf, x, y0, h, style);
        set(buf, x, y1, h, style);
    }
    for y in (y0 + 1)..y1 {
        set(buf, x0, y, v, style);
        set(buf, x1, y, v, style);
    }
}

fn set(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch);
        cell.set_style(style);
    }
}
