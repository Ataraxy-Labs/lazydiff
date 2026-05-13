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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerticalScrollbar {
    pub area: Rect,
    pub total_rows: usize,
    pub viewport_rows: usize,
    pub position: usize,
}

impl SliderState {
    pub fn geometry(self, track_size: usize) -> SliderGeometry {
        let virtual_track_size = track_size * 2;
        let range = self.max.saturating_sub(self.min);
        if range == 0 {
            return SliderGeometry {
                virtual_track_size,
                virtual_thumb_start: 0,
                virtual_thumb_size: virtual_track_size,
            };
        }

        let viewport_size = self.viewport_size.max(1);
        let content_size = range + viewport_size;
        let min_thumb_size = virtual_track_size.min(2);
        let virtual_thumb_size = ((virtual_track_size * viewport_size) / content_size)
            .max(min_thumb_size)
            .min(virtual_track_size);
        let value = self.value.saturating_sub(self.min).min(range);
        let virtual_thumb_start = (((virtual_track_size.saturating_sub(virtual_thumb_size)) * value)
            as f64
            / range as f64)
            .round() as usize;

        SliderGeometry {
            virtual_track_size,
            virtual_thumb_start,
            virtual_thumb_size,
        }
    }

    pub fn value_from_virtual_thumb_start(
        self,
        track_size: usize,
        virtual_thumb_start: usize,
    ) -> usize {
        let geometry = self.geometry(track_size);
        let max_thumb_start = geometry
            .virtual_track_size
            .saturating_sub(geometry.virtual_thumb_size);
        let range = self.max.saturating_sub(self.min);
        if range == 0 || max_thumb_start == 0 {
            return self.min;
        }

        self.min
            + ((virtual_thumb_start.min(max_thumb_start) * range) as f64 / max_thumb_start as f64)
                .round() as usize
    }
}

impl VerticalScrollbar {
    pub fn new(area: Rect, total_rows: usize, viewport_rows: usize, position: usize) -> Self {
        Self {
            area,
            total_rows,
            viewport_rows,
            position,
        }
    }

    pub fn is_visible(self) -> bool {
        !self.area.is_empty() && self.total_rows > self.viewport_rows && self.viewport_rows > 0
    }

    pub fn hit(self, column: u16, row: u16) -> bool {
        self.is_visible()
            && self.area.width > 0
            && column == self.area.right().saturating_sub(1)
            && row >= self.area.y
            && row < self.area.bottom()
    }

    pub fn thumb_hit(self, row: u16) -> bool {
        if !self.is_visible() || row < self.area.y || row >= self.area.bottom() {
            return false;
        }
        let geometry = self.slider().geometry(self.area.height.max(1) as usize);
        let thumb_start = geometry.virtual_thumb_start;
        let thumb_end = thumb_start + geometry.virtual_thumb_size;
        let virtual_row_start = row.saturating_sub(self.area.y) as usize * 2;
        let virtual_row_end = virtual_row_start + 2;
        virtual_row_start < thumb_end && virtual_row_end > thumb_start
    }

    pub fn slider(self) -> SliderState {
        let max = self.total_rows.saturating_sub(self.viewport_rows);
        SliderState {
            value: self.position.min(max),
            min: 0,
            max,
            viewport_size: self.viewport_rows.max(1),
        }
    }

    pub fn drag_offset_virtual(self, row: u16) -> usize {
        let slider = self.slider();
        let thumb_start = slider
            .geometry(self.area.height.max(1) as usize)
            .virtual_thumb_start;
        let local_row = row.saturating_sub(self.area.y) as usize;
        let virtual_mouse = local_row.min(self.area.height as usize) * 2;
        virtual_mouse
            .saturating_sub(thumb_start)
            .min(slider.geometry(self.area.height.max(1) as usize).virtual_thumb_size)
    }

    pub fn value_from_drag(self, row: u16, drag_offset_virtual: usize) -> usize {
        let track_height = self.area.height.max(1) as usize;
        let slider = self.slider();
        if slider.max == 0 {
            return 0;
        }
        let geometry = slider.geometry(track_height);
        let max_thumb_start = geometry
            .virtual_track_size
            .saturating_sub(geometry.virtual_thumb_size);
        let local_row = row.saturating_sub(self.area.y) as usize;
        let virtual_mouse = local_row.min(track_height) * 2;
        let desired_thumb_start = virtual_mouse
            .saturating_sub(drag_offset_virtual)
            .min(max_thumb_start);
        slider
            .value_from_virtual_thumb_start(track_height, desired_thumb_start)
            .min(slider.max)
    }
}

pub fn render_scrollbar(
    area: Rect,
    buf: &mut Buffer,
    total_rows: usize,
    viewport_height: usize,
    scroll_y: usize,
) {
    if area.is_empty() {
        return;
    }

    if total_rows <= viewport_height || total_rows == 0 {
        return;
    }

    let track_style = Style::new()
        .fg(Color::Rgb(56, 56, 58))
        .bg(Color::Rgb(37, 37, 39));
    let x = area.right().saturating_sub(1);
    for y in area.top()..area.bottom() {
        buf[(x, y)].set_symbol("▕").set_style(track_style);
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
    let thumb_style = Style::new()
        .fg(Color::Rgb(154, 158, 163))
        .bg(Color::Rgb(37, 37, 39));

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
            buf[(x, y)].set_symbol(symbol).set_style(thumb_style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_uses_opentui_half_cell_geometry() {
        let state = SliderState {
            value: 50,
            min: 0,
            max: 100,
            viewport_size: 20,
        };
        let geometry = state.geometry(10);

        assert_eq!(geometry.virtual_track_size, 20);
        assert!(geometry.virtual_thumb_size > 0);
        assert!(
            geometry.virtual_thumb_start
                <= geometry.virtual_track_size - geometry.virtual_thumb_size
        );
    }

    #[test]
    fn vertical_scrollbar_drag_preserves_grab_offset() {
        let scrollbar = VerticalScrollbar::new(Rect::new(10, 5, 1, 10), 120, 20, 50);
        let offset = scrollbar.drag_offset_virtual(9);

        assert!(scrollbar.value_from_drag(9, offset).abs_diff(50) <= 3);
        assert!(scrollbar.value_from_drag(14, offset) > 50);
    }
}
