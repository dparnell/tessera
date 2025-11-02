//! Core module for syntax-highlighted text editing in Tessera UI.
//!
//! This mirrors `text_edit_core`, but allows injecting a highlighter callback that can
//! update the buffer (e.g., via cosmic-text's syntect integration) before shaping.

use std::sync::Arc;
use cosmic_text::Edit;
use parking_lot::RwLock;
use tessera_ui::{ComputedData, DimensionValue, Px, PxPosition, tessera};

use crate::pipelines::{TextCommand, TextConstraint};
use crate::text_edit_core::{RectDef, TextEditorState};
use crate::text_edit_core::cursor::{self, CURSOR_WIDRH};
use glyphon;

/// Optional highlighter callback type. If provided, it will be invoked on each measure pass
/// before shaping to apply syntax highlighting.
pub type HighlighterCb = Arc<dyn Fn() + Send + Sync>;

fn compute_selection_rects(editor: &glyphon::Editor) -> Vec<RectDef> {
    let mut selection_rects: Vec<RectDef> = Vec::new();
    let (selection_start, selection_end) = editor.selection_bounds().unwrap_or_default();

    editor.with_buffer(|buffer| {
        for run in buffer.layout_runs() {
            let line_top = Px(run.line_top as i32);
            let line_height = Px(run.line_height as i32);

            if let Some((x, w)) = run.highlight(selection_start, selection_end) {
                selection_rects.push(RectDef {
                    x: Px(x as i32),
                    y: line_top,
                    width: Px(w as i32),
                    height: line_height,
                });
            }
        }
    });

    selection_rects
}

fn clip_and_take_visible(rects: Vec<RectDef>, visible_x1: Px, visible_y1: Px) -> Vec<RectDef> {
    let visible_x0 = Px(0);
    let visible_y0 = Px(0);

    rects
        .into_iter()
        .filter_map(|mut rect| {
            let rect_x1 = rect.x + rect.width;
            let rect_y1 = rect.y + rect.height;
            if rect_x1 <= visible_x0
                || rect.y >= visible_y1
                || rect.x >= visible_x1
                || rect_y1 <= visible_y0
            {
                None
            } else {
                let new_x = rect.x.max(visible_x0);
                let new_y = rect.y.max(visible_y0);
                let new_x1 = rect_x1.min(visible_x1);
                let new_y1 = rect_y1.min(visible_y1);
                rect.x = new_x;
                rect.y = new_y;
                rect.width = (new_x1 - new_x).max(Px(0));
                rect.height = (new_y1 - new_y).max(Px(0));
                Some(rect)
            }
        })
        .collect()
}

#[tessera]
pub fn syntax_edit_core(state: Arc<RwLock<TextEditorState>>, highlighter: Option<HighlighterCb>) {
    {
        let state_clone = state.clone();
        let highlighter_clone = highlighter.clone();
        measure(Box::new(move |input| {
            input.enable_clipping();

            let max_width_pixels: Option<Px> = match input.parent_constraint.width {
                DimensionValue::Fixed(w) => Some(w),
                DimensionValue::Wrap { max, .. } => max,
                DimensionValue::Fill { max, .. } => max,
            };
            let max_height_pixels: Option<Px> = match input.parent_constraint.height {
                DimensionValue::Fixed(h) => Some(h),
                DimensionValue::Wrap { max, .. } => max,
                DimensionValue::Fill { max, .. } => max,
            };

            // Run optional highlighter before shaping/layout
            if let Some(cb) = &highlighter_clone {
                cb();
            }

            let text_data = state_clone.write().text_data(TextConstraint {
                max_width: max_width_pixels.map(|px| px.to_f32()),
                max_height: max_height_pixels.map(|px| px.to_f32()),
            });

            let mut selection_rects = compute_selection_rects(state_clone.read().editor());
            let selection_rects_len = selection_rects.len();
            for (i, rect_def) in selection_rects.iter().enumerate() {
                if let Some(rect_node_id) = input.children_ids.get(i).copied() {
                    input.measure_child(rect_node_id, input.parent_constraint)?;
                    input.place_child(rect_node_id, PxPosition::new(rect_def.x, rect_def.y));
                }
            }

            let visible_x1 = max_width_pixels.unwrap_or(Px(i32::MAX));
            let visible_y1 = max_height_pixels.unwrap_or(Px(i32::MAX));
            selection_rects = clip_and_take_visible(selection_rects, visible_x1, visible_y1);
            state_clone.write().current_selection_rects = selection_rects;

            if let Some(cursor_pos_raw) = state_clone.read().editor().cursor_position() {
                let cursor_pos = PxPosition::new(Px(cursor_pos_raw.0), Px(cursor_pos_raw.1));
                let cursor_node_index = selection_rects_len;
                if let Some(cursor_node_id) = input.children_ids.get(cursor_node_index).copied() {
                    input.measure_child(cursor_node_id, input.parent_constraint)?;
                    input.place_child(cursor_node_id, cursor_pos);
                }
            }

            let drawable = TextCommand { data: text_data.clone() };
            input.metadata_mut().push_draw_command(drawable);

            let constrained_height = if let Some(max_h) = max_height_pixels { text_data.size[1].min(max_h.abs()) } else { text_data.size[1] };

            Ok(ComputedData {
                width: Px::from(text_data.size[0]) + CURSOR_WIDRH.to_px(),
                height: constrained_height.into(),
            })
        }));
    }

    // selection highlight quads
    {
        let (rects, color) = { let guard = state.read(); (guard.current_selection_rects.clone(), guard.selection_color) };
        for def in rects { crate::selection_highlight_rect::selection_highlight_rect(def.width, def.height, color); }
    }

    if state.read().focus_handler().is_focused() {
        cursor::cursor(state.read().line_height(), state.read().blink_timer());
    }
}
