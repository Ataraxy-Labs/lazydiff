use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliderState {
    pub value: usize,
    pub min: usize,
    pub max: usize,
    pub viewport_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliderGeometry {
    pub virtual_track_size: usize,
    pub virtual_thumb_start: usize,
    pub virtual_thumb_size: usize,
}

impl SliderState {
    pub fn geometry(self, track_size: usize) -> SliderGeometry {
        let virtual_track_size = track_size * 2;
        let range = self.max.saturating_sub(self.min);
        if range == 0 {
            return SliderGeometry { virtual_track_size, virtual_thumb_start: 0, virtual_thumb_size: virtual_track_size };
        }

        let viewport_size = self.viewport_size.max(1);
        let content_size = range + viewport_size;
        let min_thumb_size = virtual_track_size.min(2);
        let virtual_thumb_size = ((virtual_track_size * viewport_size) / content_size)
            .max(min_thumb_size)
            .min(virtual_track_size);
        let value = self.value.saturating_sub(self.min).min(range);
        let virtual_thumb_start = (((virtual_track_size.saturating_sub(virtual_thumb_size)) * value) as f64 / range as f64).round() as usize;

        SliderGeometry { virtual_track_size, virtual_thumb_start, virtual_thumb_size }
    }

    pub fn value_from_virtual_thumb_start(self, track_size: usize, virtual_thumb_start: usize) -> usize {
        let geometry = self.geometry(track_size);
        let max_thumb_start = geometry.virtual_track_size.saturating_sub(geometry.virtual_thumb_size);
        let range = self.max.saturating_sub(self.min);
        if range == 0 || max_thumb_start == 0 {
            return self.min;
        }

        self.min + ((virtual_thumb_start.min(max_thumb_start) * range) as f64 / max_thumb_start as f64).round() as usize
    }
}

pub(crate) fn render_scrollbar(area: Rect, buf: &mut Buffer, total_rows: usize, viewport_height: usize, scroll_y: usize) {
    if area.is_empty() {
        return;
    }

    if total_rows <= viewport_height || total_rows == 0 {
        return;
    }

    let track_style = Style::new().fg(Color::Rgb(56, 56, 58)).bg(Color::Rgb(37, 37, 39));
    for y in area.top()..area.bottom() {
        buf[(area.x, y)].set_symbol("▕").set_style(track_style);
    }

    let height = area.height as usize;
    let state = SliderState {
        value: scroll_y,
        min: 0,
        max: total_rows.saturating_sub(viewport_height),
        viewport_size: viewport_height,
    };
    let geometry = state.geometry(height);
    let thumb_start = geometry.virtual_thumb_start;
    let thumb_end = thumb_start + geometry.virtual_thumb_size;
    let thumb_style = Style::new().fg(Color::Rgb(154, 158, 163)).bg(Color::Rgb(37, 37, 39));

    let start_cell = thumb_start / 2;
    let end_cell = thumb_end.div_ceil(2).saturating_sub(1);
    for real_y in start_cell..=end_cell.min(height.saturating_sub(1)) {
        let virtual_cell_start = real_y * 2;
        let virtual_cell_end = virtual_cell_start + 2;
        let covered_start = thumb_start.max(virtual_cell_start);
        let covered_end = thumb_end.min(virtual_cell_end);
        let coverage = covered_end.saturating_sub(covered_start);
        let symbol = if coverage >= 2 {
            "▐"
        } else if coverage > 0 && covered_start == virtual_cell_start {
            "▝"
        } else if coverage > 0 {
            "▗"
        } else {
            "▕"
        };
        let y = area.y + real_y as u16;
        if y < area.bottom() {
            buf[(area.x, y)].set_symbol(symbol).set_style(thumb_style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_uses_opentui_half_cell_geometry() {
        let state = SliderState { value: 50, min: 0, max: 100, viewport_size: 20 };
        let geometry = state.geometry(10);

        assert_eq!(geometry.virtual_track_size, 20);
        assert!(geometry.virtual_thumb_size > 0);
        assert!(geometry.virtual_thumb_start <= geometry.virtual_track_size - geometry.virtual_thumb_size);
    }
}
