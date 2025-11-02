use std::sync::Arc;

use parking_lot::RwLock;
use tessera_ui::{Color, DimensionValue, Dp, shard, tessera};
use tessera_ui_basic_components::{
    column::{ColumnArgsBuilder, column},
    scrollable::{ScrollableArgsBuilder, ScrollableState, scrollable},
    spacer::spacer,
    surface::{SurfaceArgsBuilder, surface},
    text::{TextArgsBuilder, text},
    syntax_editor::{SyntaxEditorArgsBuilder, SyntaxTextEditorState, syntax_editor},
};

#[derive(Clone)]
struct SyntaxEditorShowcaseState {
    scrollable_state: Arc<ScrollableState>,
    editor_state: Arc<RwLock<SyntaxTextEditorState>>,
}

impl Default for SyntaxEditorShowcaseState {
    fn default() -> Self {
        Self {
            scrollable_state: Default::default(),
            editor_state: Arc::new(RwLock::new(SyntaxTextEditorState::new(Dp(22.0), None))),
        }
    }
}

#[tessera]
#[shard]
pub fn syntax_editor_showcase(#[state] state: SyntaxEditorShowcaseState) {
    surface(
        SurfaceArgsBuilder::default()
            .width(DimensionValue::FILLED)
            .height(DimensionValue::FILLED)
            .style(Color::WHITE.into())
            .build()
            .unwrap(),
        None,
        move || {
            scrollable(
                ScrollableArgsBuilder::default()
                    .width(DimensionValue::FILLED)
                    .build()
                    .unwrap(),
                state.scrollable_state.clone(),
                move || {
                    surface(
                        SurfaceArgsBuilder::default()
                            .style(Color::WHITE.into())
                            .padding(Dp(25.0))
                            .width(DimensionValue::FILLED)
                            .build()
                            .unwrap(),
                        None,
                        move || {
                            test_content(state);
                        },
                    );
                },
            )
        },
    );
}

#[tessera]
fn test_content(state: Arc<SyntaxEditorShowcaseState>) {
    column(
        ColumnArgsBuilder::default()
            .width(DimensionValue::FILLED)
            .build()
            .unwrap(),
        |scope| {
            scope.child(|| {
                text(
                    TextArgsBuilder::default()
                        .text("Syntax Editor Showcase")
                        .size(Dp(20.0))
                        .build()
                        .unwrap(),
                )
            });

            scope.child(|| spacer(Dp(10.0)));

            scope.child(move || {
                syntax_editor(
                    SyntaxEditorArgsBuilder::default()
                        .width(DimensionValue::FILLED)
                        .height(Dp(300.0))
                        .theme_name("base16-eighties.dark".to_string())
                        .file_extension(Some("rs".to_string()))
                        .on_change(Arc::new(move |new_value| new_value))
                        .build()
                        .unwrap(),
                    state.editor_state.clone(),
                );
            });
        },
    )
}
