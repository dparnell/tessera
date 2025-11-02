//! Syntax-highlighted text editor component for Tessera UI, powered by cosmic-text + syntect.
//!
//! This component mirrors the public API and behavior of `text_editor`, but adds syntax
//! highlighting via the `cosmic-text` syntect integration. It reuses `TextEditorState` and the
//! same event handling as `text_editor`, injecting a highlighter before shaping.

use std::sync::Arc;

use derive_builder::Builder;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use tessera_ui::{Color, CursorEventContent, DimensionValue, Dp, Px, PxPosition, ImeRequest, winit, tessera};

use crate::{
    pipelines::write_font_system,
    pos_misc::is_position_in_component,
    shape_def::Shape,
    surface::{SurfaceArgsBuilder, surface},
    syntax_edit_core::{syntax_edit_core, HighlighterCb},
    text_edit_core::TextEditorState,
};

use cosmic_text::{SyntaxEditor as CtSyntaxEditor, SyntaxSystem};
use glyphon::{Action, Edit};

/// Global syntax system (themes + syntaxes)
pub static SYNTAX_SYSTEM: Lazy<SyntaxSystem> = Lazy::new(SyntaxSystem::new);

/// Re-export the shared text editor state so callers can reuse it
pub use crate::text_edit_core::TextEditorState as SyntaxTextEditorState;

/// Arguments mirroring `TextEditorArgs` with an extra `theme_name` and `file_extension` fields.
#[derive(Builder, Clone)]
#[builder(pattern = "owned")]
pub struct SyntaxEditorArgs {
    #[builder(default = "DimensionValue::WRAP", setter(into))]
    pub width: DimensionValue,
    #[builder(default = "DimensionValue::WRAP", setter(into))]
    pub height: DimensionValue,

    /// Called when the text content changes. If not set the editor will not accept input.
    #[builder(default = "Arc::new(|_| { String::new() })")]
    pub on_change: Arc<dyn Fn(String) -> String + Send + Sync>,

    #[builder(default = "None")]
    pub min_width: Option<Dp>,
    #[builder(default = "None")]
    pub min_height: Option<Dp>,

    #[builder(default = "None")]
    pub background_color: Option<Color>,
    #[builder(default = "Dp(1.0)")]
    pub border_width: Dp,
    #[builder(default = "None")]
    pub border_color: Option<Color>,

    #[builder(default = "Shape::RoundedRectangle { top_left: Dp(4.0), top_right: Dp(4.0), bottom_right: Dp(4.0), bottom_left: Dp(4.0), g2_k_value: 3.0 }")]
    pub shape: Shape,

    #[builder(default = "Dp(5.0)")]
    pub padding: Dp,
    #[builder(default = "None")]
    pub focus_border_color: Option<Color>,
    #[builder(default = "None")]
    pub focus_background_color: Option<Color>,

    /// Selection color for highlighting (overrides theme-derived behavior).
    #[builder(default = "Some(Color::new(0.5, 0.7, 1.0, 0.4))")]
    pub selection_color: Option<Color>,

    /// Syntect theme name. Defaults to "base16-eighties.dark".
    #[builder(default = "\"base16-eighties.dark\".to_string()")]
    pub theme_name: String,

    /// The file extension of the file being edited.
    #[builder(default = "None")]
    pub file_extension: Option<String>,
}

impl Default for SyntaxEditorArgs { fn default() -> Self { SyntaxEditorArgsBuilder::default().build().unwrap() } }

/// Create surface arguments based on editor configuration and state (mirrors text_editor)
fn create_surface_args(args: &SyntaxEditorArgs, state: &Arc<RwLock<TextEditorState>>) -> crate::surface::SurfaceArgs {
    let style = if args.border_width.to_pixels_f32() > 0.0 {
        crate::surface::SurfaceStyle::FilledOutlined {
            fill_color: determine_background_color(args, state),
            border_color: determine_border_color(args, state).unwrap(),
            border_width: args.border_width,
        }
    } else {
        crate::surface::SurfaceStyle::Filled {
            color: determine_background_color(args, state),
        }
    };

    SurfaceArgsBuilder::default()
        .style(style)
        .shape(args.shape)
        .padding(args.padding)
        .width(args.width)
        .height(args.height)
        .build()
        .unwrap()
}

fn determine_background_color(args: &SyntaxEditorArgs, state: &Arc<RwLock<TextEditorState>>) -> Color {
    if state.read().focus_handler().is_focused() {
        args.focus_background_color
            .or(args.background_color)
            .unwrap_or(Color::WHITE)
    } else {
        args.background_color
            .unwrap_or(Color::new(0.95, 0.95, 0.95, 1.0))
    }
}

fn determine_border_color(args: &SyntaxEditorArgs, state: &Arc<RwLock<TextEditorState>>) -> Option<Color> {
    if state.read().focus_handler().is_focused() {
        args.focus_border_color
            .or(args.border_color)
            .or(Some(Color::new(0.0, 0.5, 1.0, 1.0)))
    } else {
        args.border_color.or(Some(Color::new(0.7, 0.7, 0.7, 1.0)))
    }
}

/// A syntax-highlighting text editor, mirroring `text_editor` API and behavior.
#[tessera]
pub fn syntax_editor(args: impl Into<SyntaxEditorArgs>, state: Arc<RwLock<TextEditorState>>) {
    let editor_args: SyntaxEditorArgs = args.into();
    let on_change = editor_args.on_change.clone();

    // Update the state with the selection color from args
    if let Some(selection_color) = editor_args.selection_color {
        state.write().set_selection_color(selection_color);
    }

    // Prepare highlighter closure that applies syntect highlighting before shaping
    let theme = editor_args.theme_name.clone();
    let file_extension = editor_args.file_extension.clone();
    let state_for_highlight = state.clone();
    let highlighter: HighlighterCb = Arc::new(move || {
        // Borrow the buffer mutably and run syntect highlighting
        // Any failure to find the theme is silently ignored (no highlighting)
        state_for_highlight.write().editor_mut().with_buffer_mut(|buffer| {
            if let Some(mut se) = CtSyntaxEditor::new(buffer, &SYNTAX_SYSTEM, &theme) {
                if let Some(file_extension) = &file_extension {
                    se.syntax_by_extension(file_extension.as_str());
                }
                se.shape_as_needed(&mut write_font_system(), false);
            }
        });
    });

    // surface layer - provides visual container and minimum size guarantee
    {
        let state_for_surface = state.clone();
        let args_for_surface = editor_args.clone();
        let highlighter_for_surface = Some(highlighter.clone());
        surface(
            create_surface_args(&args_for_surface, &state_for_surface),
            None,
            move || {
                syntax_edit_core(state_for_surface.clone(), highlighter_for_surface.clone());
            },
        );
    }

    // Event handling at the outermost layer - identical to text_editor
    let state_for_handler = state.clone();
    input_handler(Box::new(move |input| {
        let size = input.computed_data; // full surface size
        let cursor_pos_option = input.cursor_position_rel;
        let is_cursor_in_editor = cursor_pos_option
            .map(|pos| is_position_in_component(size, pos))
            .unwrap_or(false);

        // Set text input cursor when hovering
        if is_cursor_in_editor {
            input.requests.cursor_icon = winit::window::CursorIcon::Text;
        }

        // Handle click/drag/scroll events when cursor is in editor
        if is_cursor_in_editor {
            // Mouse pressed events
            let click_events: Vec<_> = input
                .cursor_events
                .iter()
                .filter(|event| matches!(event.content, CursorEventContent::Pressed(_)))
                .collect();

            // Mouse released events (end of drag)
            let release_events: Vec<_> = input
                .cursor_events
                .iter()
                .filter(|event| matches!(event.content, CursorEventContent::Released(_)))
                .collect();

            if !click_events.is_empty() {
                // Ensure focus
                if !state_for_handler.read().focus_handler().is_focused() {
                    state_for_handler.write().focus_handler_mut().request_focus();
                }

                if let Some(cursor_pos) = cursor_pos_option {
                    // Convert to text-relative position (account for padding and border)
                    let padding_px: Px = editor_args.padding.into();
                    let border_width_px = Px(editor_args.border_width.to_pixels_u32() as i32);

                    let text_relative_x_px = cursor_pos.x - padding_px - border_width_px;
                    let text_relative_y_px = cursor_pos.y - padding_px - border_width_px;

                    if text_relative_x_px >= Px(0) && text_relative_y_px >= Px(0) {
                        let text_relative_pos = PxPosition::new(text_relative_x_px, text_relative_y_px);
                        let click_type = state_for_handler
                            .write()
                            .handle_click(text_relative_pos, click_events[0].timestamp);

                        match click_type {
                            crate::text_edit_core::ClickType::Single => {
                                state_for_handler.write().editor_mut().action(
                                    &mut write_font_system(),
                                    Action::Click { x: text_relative_pos.x.0, y: text_relative_pos.y.0 },
                                );
                            }
                            crate::text_edit_core::ClickType::Double => {
                                state_for_handler.write().editor_mut().action(
                                    &mut write_font_system(),
                                    Action::DoubleClick { x: text_relative_pos.x.0, y: text_relative_pos.y.0 },
                                );
                            }
                            crate::text_edit_core::ClickType::Triple => {
                                state_for_handler.write().editor_mut().action(
                                    &mut write_font_system(),
                                    Action::TripleClick { x: text_relative_pos.x.0, y: text_relative_pos.y.0 },
                                );
                            }
                        }

                        state_for_handler.write().start_drag();
                    }
                }
            }

            // Drag selection if dragging
            if state_for_handler.read().is_dragging()
                && let Some(cursor_pos) = cursor_pos_option
            {
                let padding_px: Px = editor_args.padding.into();
                let border_width_px = Px(editor_args.border_width.to_pixels_u32() as i32);

                let text_relative_x_px = cursor_pos.x - padding_px - border_width_px;
                let text_relative_y_px = cursor_pos.y - padding_px - border_width_px;

                if text_relative_x_px >= Px(0) && text_relative_y_px >= Px(0) {
                    let current_pos_px = PxPosition::new(text_relative_x_px, text_relative_y_px);
                    let last_pos_px = state_for_handler.read().last_click_position();

                    if last_pos_px != Some(current_pos_px) {
                        state_for_handler.write().editor_mut().action(
                            &mut write_font_system(),
                            Action::Drag { x: current_pos_px.x.0, y: current_pos_px.y.0 },
                        );
                        state_for_handler.write().update_last_click_position(current_pos_px);
                    }
                }
            }

            // Mouse release ends drag
            if !release_events.is_empty() {
                state_for_handler.write().stop_drag();
            }

            // Handle scroll from cursor events
            let scroll_events: Vec<_> = input
                .cursor_events
                .iter()
                .filter_map(|event| match &event.content {
                    CursorEventContent::Scroll(scroll_event) => Some(scroll_event),
                    _ => None,
                })
                .collect();

            if state_for_handler.read().focus_handler().is_focused() {
                for scroll_event in scroll_events {
                    let scroll = -scroll_event.delta_y;
                    let action = glyphon::Action::Scroll { pixels: scroll };
                    state_for_handler
                        .write()
                        .editor_mut()
                        .action(&mut write_font_system(), action);
                }
            }

            // If focused, block cursor events propagation
            if state_for_handler.read().focus_handler().is_focused() {
                input.cursor_events.clear();
            }
        }

        // Keyboard + IME (only when focused)
        if state_for_handler.read().focus_handler().is_focused() {
            let is_ctrl = input.key_modifiers.control_key() || input.key_modifiers.super_key();

            // Ctrl+A select all special-case
            let select_all_event_index = input.keyboard_events.iter().position(|key_event| {
                if let winit::keyboard::Key::Character(s) = &key_event.logical_key {
                    is_ctrl && s.to_lowercase() == "a" && key_event.state == winit::event::ElementState::Pressed
                } else { false }
            });

            if let Some(_index) = select_all_event_index {
                let mut state = state_for_handler.write();
                let editor = state.editor_mut();
                editor.set_cursor(glyphon::Cursor::new(0, 0));
                editor.set_selection(glyphon::cosmic_text::Selection::Normal(glyphon::Cursor::new(0, 0)));
                editor.action(&mut write_font_system(), glyphon::Action::Motion(glyphon::cosmic_text::Motion::BufferEnd));
            } else {
                let mut all_actions = Vec::new();
                {
                    let mut state = state_for_handler.write();
                    for key_event in input.keyboard_events.iter().cloned() {
                        if let Some(actions) = state.map_key_event_to_action(key_event, input.key_modifiers, input.clipboard) {
                            all_actions.extend(actions);
                        }
                    }
                }
                if !all_actions.is_empty() {
                    let mut state = state_for_handler.write();
                    for action in all_actions { handle_action(&mut state, action, on_change.clone()); }
                }
            }

            // Block all keyboard events
            input.keyboard_events.clear();

            // IME events
            let ime_events: Vec<_> = input.ime_events.drain(..).collect();
            for event in ime_events {
                let mut state = state_for_handler.write();
                match event {
                    winit::event::Ime::Commit(text) => {
                        if let Some(preedit_text) = state.preedit_string.take() {
                            for _ in 0..preedit_text.chars().count() { handle_action(&mut state, Action::Backspace, on_change.clone()); }
                        }
                        for c in text.chars() { handle_action(&mut state, Action::Insert(c), on_change.clone()); }
                    }
                    winit::event::Ime::Preedit(text, _cursor_offset) => {
                        if let Some(old_preedit) = state.preedit_string.take() {
                            for _ in 0..old_preedit.chars().count() { handle_action(&mut state, Action::Backspace, on_change.clone()); }
                        }
                        for c in text.chars() { handle_action(&mut state, Action::Insert(c), on_change.clone()); }
                        state.preedit_string = Some(text.to_string());
                    }
                    _ => {}
                }
            }

            // Request IME window
            input.requests.ime_request = Some(ImeRequest::new(size.into()));
        }
    }));
}

fn get_editor_content(editor: &glyphon::Editor) -> String {
    let mut content = String::new();
    editor.with_buffer(|buffer| {
        for line in &buffer.lines {
            content.push_str(line.text());
            content.push('\n');
        }
    });
    if content.ends_with('\n') {
        content.pop();
    }
    content
}

// Helper copied from text_editor.rs to apply on_change roundtrip per action
fn handle_action(
    state: &mut TextEditorState,
    action: Action,
    on_change: Arc<dyn Fn(String) -> String + Send + Sync>,
) {
    let mut new_editor = state.editor().clone();

    let mut new_buffer = None;
    match new_editor.buffer_ref_mut() {
        glyphon::cosmic_text::BufferRef::Owned(_) => {}
        glyphon::cosmic_text::BufferRef::Borrowed(buffer) => {
            new_buffer = Some(buffer.clone());
        }
        glyphon::cosmic_text::BufferRef::Arc(buffer) => {
            new_buffer = Some((**buffer).clone());
        }
    }
    if let Some(buffer) = new_buffer {
        *new_editor.buffer_ref_mut() = glyphon::cosmic_text::BufferRef::Owned(buffer);
    }

    new_editor.action(&mut write_font_system(), action);
    let content_after_action = get_editor_content(&new_editor);

    state.editor_mut().action(&mut write_font_system(), action);
    let new_content = on_change(content_after_action);

    state.editor_mut().set_text_reactive(
        &new_content,
        &mut write_font_system(),
        &glyphon::Attrs::new().family(glyphon::fontdb::Family::SansSerif),
    );
}

/// Convenience constructors, mirroring `TextEditorArgs` styles
impl SyntaxEditorArgs {
    pub fn simple() -> Self {
        SyntaxEditorArgsBuilder::default()
            .min_width(Some(Dp(120.0)))
            .background_color(Some(Color::WHITE))
            .border_width(Dp(1.0))
            .border_color(Some(Color::new(0.7, 0.7, 0.7, 1.0)))
            .shape(Shape::RoundedRectangle {
                top_left: Dp(0.0),
                top_right: Dp(0.0),
                bottom_right: Dp(0.0),
                bottom_left: Dp(0.0),
                g2_k_value: 3.0,
            })
            .build()
            .unwrap()
    }
    pub fn outlined() -> Self {
        Self::simple()
            .with_border_width(Dp(1.0))
            .with_focus_border_color(Color::new(0.0, 0.5, 1.0, 1.0))
    }
    pub fn minimal() -> Self {
        SyntaxEditorArgsBuilder::default()
            .min_width(Some(Dp(120.0)))
            .background_color(Some(Color::WHITE))
            .shape(Shape::RoundedRectangle {
                top_left: Dp(0.0),
                top_right: Dp(0.0),
                bottom_right: Dp(0.0),
                bottom_left: Dp(0.0),
                g2_k_value: 3.0,
            })
            .build()
            .unwrap()
    }
}

impl SyntaxEditorArgs {
    pub fn with_width(mut self, width: DimensionValue) -> Self { self.width = width; self }
    pub fn with_height(mut self, height: DimensionValue) -> Self { self.height = height; self }
    pub fn with_min_width(mut self, min_width: Dp) -> Self { self.min_width = Some(min_width); self }
    pub fn with_min_height(mut self, min_height: Dp) -> Self { self.min_height = Some(min_height); self }
    pub fn with_background_color(mut self, color: Color) -> Self { self.background_color = Some(color); self }
    pub fn with_border_width(mut self, width: Dp) -> Self { self.border_width = width; self }
    pub fn with_border_color(mut self, color: Color) -> Self { self.border_color = Some(color); self }
    pub fn with_shape(mut self, shape: Shape) -> Self { self.shape = shape; self }
    pub fn with_padding(mut self, padding: Dp) -> Self { self.padding = padding; self }
    pub fn with_focus_border_color(mut self, color: Color) -> Self { self.focus_border_color = Some(color); self }
    pub fn with_focus_background_color(mut self, color: Color) -> Self { self.focus_background_color = Some(color); self }
    pub fn with_selection_color(mut self, color: Color) -> Self { self.selection_color = Some(color); self }
    pub fn with_theme_name(mut self, theme: impl Into<String>) -> Self { self.theme_name = theme.into(); self }
    pub fn with_file_extension(mut self, extention: impl Into<String>) -> Self { self.file_extension = Some(extention.into()); self }
}
