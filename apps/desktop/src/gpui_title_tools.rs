//! System-wide file, interchange and clipboard commands on the title bar.
use super::*;

/// `open_menu` value for the popup opened by the Recent Files button.
pub(super) const RECENT_MENU: &str = "Recent Files";
const RECENT_COMMAND: &str = "file.open_recent";

/// Editors and prompts start below the title bar, so its buttons stay reachable
/// while they are open. Their commands must not act behind them.
pub(super) fn blocked(shell: &MeshcraftShell) -> bool {
    shell.scripts.editor.is_some()
        || shell.ifc.editor.is_some()
        || materials::modal_open(shell)
        || shell.session.native_prototype_deletion_prompt().is_some()
        || shell.session.native_document_replacement_prompt().is_some()
}

/// Home keeps its own toggle at the start of the title bar.
fn shown(id: &str) -> bool {
    id != "file.home"
}

#[cfg(test)]
pub(super) fn command_ids() -> Vec<&'static str> {
    crate::functional_ui::title_bar_sections()
        .into_iter()
        .flat_map(|section| section.commands.iter().map(|(id, _)| *id))
        .filter(|id| shown(id))
        .collect()
}

/// Width of the search cluster: it gives way to the title, down to a usable minimum.
fn search_width(window_width: f32) -> f32 {
    (window_width * 0.18).clamp(150., 340.)
}

/// Command search box with its search and list buttons, right-aligned on the title bar.
pub(super) fn search(
    shell: &MeshcraftShell,
    window: &Window,
    cx: &mut Context<MeshcraftShell>,
) -> gpui::AnyElement {
    let blocked = blocked(shell);
    let open = shell.command_palette_open;
    let input = shell
        .palette_input
        .as_ref()
        .expect("prepared palette input");
    let probe = |key: &str| workspace_controls::bounds_probe(shell, key.to_owned());
    div()
        .id("search/cluster")
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(2.))
        .ml(px(6.))
        .w(px(search_width(f32::from(window.viewport_size().width))))
        .occlude()
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .relative()
                .flex_1()
                .min_w(px(0.))
                // A disabled input can still take focus, away from the open editor.
                .when(blocked, |cell| {
                    cell.child(
                        div()
                            .h(px(20.))
                            .px(px(6.))
                            .text_xs()
                            .truncate()
                            .text_color(rgb(0x6f7c8a))
                            .child("Search commands…"),
                    )
                })
                .when(!blocked, |cell| {
                    cell.child(Input::new(input).xsmall().w_full())
                })
                .child(probe("search/box")),
        )
        .child(
            div()
                .relative()
                .flex_none()
                .child(
                    Button::new("search/run")
                        .icon(gpui_component::IconName::Search)
                        .xsmall()
                        .ghost()
                        .disabled(blocked)
                        .tooltip("Search commands")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_command_list();
                            cx.notify();
                        })),
                )
                .child(probe("search/button")),
        )
        .child(
            div()
                .relative()
                .flex_none()
                .child(
                    Button::new("search/toggle")
                        .icon(if open {
                            gpui_component::IconName::ChevronUp
                        } else {
                            gpui_component::IconName::ChevronDown
                        })
                        .xsmall()
                        .ghost()
                        .selected(open)
                        .disabled(blocked)
                        .tooltip(if open {
                            "Hide command list"
                        } else {
                            "Show command list · Ctrl+Shift+P"
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.command_palette_open {
                                this.command_palette_open = false;
                                window.focus(&this.focus);
                            } else {
                                this.show_command_list();
                            }
                            cx.notify();
                        })),
                )
                .child(probe("search/chevron")),
        )
        .child(probe("search/cluster"))
        .into_any_element()
}

fn tooltip(label: &str, command: Option<&CommandDescriptor>) -> String {
    match command.and_then(|command| command.shortcut.as_deref()) {
        Some(shortcut) => format!("{label} · {shortcut}"),
        None => label.to_owned(),
    }
}

pub(super) fn render(shell: &MeshcraftShell, cx: &mut Context<MeshcraftShell>) -> gpui::AnyElement {
    let catalog = MeshcraftApp::command_catalog();
    let blocked = blocked(shell);
    let mut row = div()
        .id("title/tools")
        .flex()
        .flex_none()
        .items_center()
        .gap(px(2.))
        // Clicks belong to the buttons, not to the title bar's window dragging.
        .occlude()
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
    for (index, section) in crate::functional_ui::title_bar_sections()
        .into_iter()
        .enumerate()
    {
        if index > 0 {
            row = row.child(div().w(px(1.)).h(px(16.)).mx(px(3.)).bg(rgb(0x3b4654)));
        }
        for (id, label) in section.commands.iter().copied().filter(|(id, _)| shown(id)) {
            let command = catalog.iter().find(|command| command.id.as_str() == id);
            let enabled = !blocked && command.is_some_and(|c| command_enabled(&shell.session, c));
            let mut button = Button::new(SharedString::from(format!("title/button/{id}")))
                .xsmall()
                .ghost()
                .compact()
                .size(px(24.))
                .disabled(!enabled)
                .tooltip(tooltip(label, command))
                .when_some(crate::assets::gpui_icon(id), |button, icon| {
                    // There is no label to dim, so the icon shows availability.
                    button.child(
                        img(ImageSource::Render(icon))
                            .size(px(16.))
                            .opacity(if enabled { 1. } else { 0.35 }),
                    )
                });
            if id == RECENT_COMMAND {
                button = button
                    .selected(shell.open_menu == Some(RECENT_MENU))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            if enabled {
                                this.toggle_menu(RECENT_MENU);
                                cx.notify();
                            }
                        }),
                    );
            } else {
                button = button.on_click(cx.listener(move |this, _, window, cx| {
                    window.focus(&this.focus);
                    this.dispatch_with_clipboard(id, cx);
                    cx.notify();
                }));
            }
            let mut cell = div()
                .relative()
                .child(button)
                .child(workspace_controls::bounds_probe(
                    shell,
                    format!("title/{id}"),
                ));
            if id == RECENT_COMMAND {
                let store = Arc::clone(&shell.menu_bounds);
                cell = cell.child(
                    canvas(
                        move |bounds, _, _| {
                            if let Ok(mut menu_bounds) = store.lock() {
                                menu_bounds.insert(RECENT_MENU, bounds);
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .left_0()
                    .top_0()
                    .size_full(),
                );
            }
            row = row.child(cell);
        }
    }
    row.into_any_element()
}
