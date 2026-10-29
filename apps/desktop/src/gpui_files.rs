//! File menu with the component library's nested, keyboard-accessible recent menu.
use super::*;
use gpui::{DismissEvent, Focusable};
use gpui_component::menu::{PopupMenu, PopupMenuItem};

type MenuKey = (Vec<String>, Vec<(String, bool)>);

pub(super) fn prototype_deletion_dialog(
    shell: &MeshcraftShell,
    cx: &mut Context<MeshcraftShell>,
) -> Option<gpui::AnyElement> {
    let prompt = shell.session.native_prototype_deletion_prompt()?;
    let dialog = div()
        .w(px(500.))
        .p(px(18.))
        .rounded(px(6.))
        .bg(rgb(0x202832))
        .border_1()
        .border_color(rgb(0x536477))
        .shadow_lg()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(div().text_lg().child("Delete prototype and instances?"))
        .child(prompt.message())
        .child(
            div().flex().gap(px(8.)).justify_end().children(
                [("Cancel", false), ("Delete Prototype and Instances", true)]
                    .into_iter()
                    .map(|(label, confirmed)| {
                        Button::new(SharedString::from(format!("prototype-delete/{confirmed}")))
                            .label(label)
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                shell.session.native_resolve_prototype_deletion(confirmed);
                                shell.refresh_scene();
                                cx.notify();
                            }))
                    }),
            ),
        );
    Some(
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::rgba(0x00000088))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(dialog)
            .into_any_element(),
    )
}

#[derive(Default)]
pub(super) struct Ui {
    menu: Option<Entity<PopupMenu>>,
    key: Option<MenuKey>,
    subscription: Option<gpui::Subscription>,
}

pub(super) fn popup(
    shell: &mut MeshcraftShell,
    entries: Vec<(String, Option<CommandDescriptor>)>,
    bounds: gpui::Bounds<gpui::Pixels>,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) -> gpui::AnyElement {
    let recent = shell.session.native_recent_projects().to_vec();
    let enabled = shell.session.native_command_state("file.open").enabled;
    let key = (
        recent.clone(),
        entries
            .iter()
            .map(|(label, command)| {
                (
                    label.clone(),
                    command
                        .as_ref()
                        .is_some_and(|c| command_enabled(&shell.session, c)),
                )
            })
            .collect(),
    );
    if shell.files.key.as_ref() != Some(&key) {
        let focus = shell.focus.clone();
        let owner = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, |mut menu, window, cx| {
            menu = menu.action_context(focus).min_w(px(235.)).max_w(px(300.));
            let mut previous = String::new();
            for (label, command) in entries {
                let section = command
                    .as_ref()
                    .and_then(|c| c.presentation.menu_path.get(1))
                    .cloned()
                    .unwrap_or_default();
                if section != previous {
                    if !previous.is_empty() {
                        menu = menu.separator();
                    }
                    menu = menu.label(section.clone());
                    previous = section;
                }
                if command
                    .as_ref()
                    .is_some_and(|c| c.id.as_str() == "file.open_recent")
                {
                    let owner = owner.clone();
                    let recent = recent.clone();
                    menu = menu.submenu("Recent Files", window, cx, move |mut menu, _, _| {
                        menu = menu
                            .min_w(px(320.))
                            .max_w(px(480.))
                            .max_h(px(420.))
                            .scrollable(true);
                        if recent.is_empty() {
                            return menu.item(
                                PopupMenuItem::new("No recently opened files").disabled(true),
                            );
                        }
                        for path in &recent {
                            let name = crate::app::recent_project_display_name(path).to_owned();
                            let detail = path.clone();
                            let selected = path.clone();
                            let owner = owner.clone();
                            menu = menu.item(
                                PopupMenuItem::element(move |_, _| {
                                    div()
                                        .flex()
                                        .flex_col()
                                        .w(px(420.))
                                        .max_w_full()
                                        .min_w(px(0.))
                                        .py(px(3.))
                                        .child(div().truncate().child(name.clone()))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgb(0xa5adb7))
                                                .truncate()
                                                .child(detail.clone()),
                                        )
                                })
                                .disabled(!enabled)
                                .on_click(move |_, _, cx| {
                                    _ = owner.update(cx, |shell, cx| {
                                        shell.open_menu = None;
                                        let mut payload = CommandPayload::default();
                                        payload.values.insert("path".into(), json!(selected));
                                        shell.dispatch_payload("file.open", &payload);
                                        cx.notify();
                                    });
                                }),
                            );
                        }
                        menu
                    });
                } else {
                    let active = command
                        .as_ref()
                        .is_some_and(|c| command_enabled(&shell.session, c));
                    let icon = command.as_ref().and_then(command_icon);
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::element(move |_, _| {
                            labeled_command(label.clone(), icon.clone(), 201.)
                        })
                        .disabled(!active)
                        .on_click(move |_, _, cx| {
                            if let Some(command) = &command {
                                _ = owner.update(cx, |shell, cx| {
                                    shell.open_menu = None;
                                    shell.dispatch_with_clipboard(command.id.as_str(), cx);
                                    cx.notify();
                                });
                            }
                        }),
                    );
                }
            }
            menu
        });
        shell.files.subscription =
            Some(
                cx.subscribe_in(&menu, window, |shell, _, _: &DismissEvent, _, cx| {
                    shell.open_menu = None;
                    cx.notify();
                }),
            );
        menu.read(cx).focus_handle(cx).focus(window);
        shell.files.menu = Some(menu);
        shell.files.key = Some(key);
    }
    div()
        .id("file-menu-popup")
        .absolute()
        .top(bounds.origin.y + bounds.size.height + px(3.))
        .left(bounds.origin.x)
        .occlude()
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .child(
            shell
                .files
                .menu
                .as_ref()
                .expect("prepared file menu")
                .clone(),
        )
        .into_any_element()
}

pub(super) fn replacement_dialog(
    shell: &MeshcraftShell,
    cx: &mut Context<MeshcraftShell>,
) -> Option<gpui::AnyElement> {
    use crate::app::ReplacementDecision;
    let target = shell.session.native_document_replacement_prompt()?;
    let buttons = [
        ("Save", ReplacementDecision::Save),
        ("Discard", ReplacementDecision::Discard),
        ("Cancel", ReplacementDecision::Cancel),
    ]
    .into_iter()
    .map(|(label, decision)| {
        Button::new(SharedString::from(format!("replace/{label}")))
            .label(label)
            .on_click(cx.listener(move |shell, _, _, cx| {
                shell.session.native_resolve_document_replacement(decision);
                shell.refresh_scene();
                cx.notify();
            }))
    });
    let dialog = div()
        .w(px(420.))
        .p(px(18.))
        .rounded(px(6.))
        .bg(rgb(0x202832))
        .border_1()
        .border_color(rgb(0x536477))
        .shadow_lg()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(div().text_lg().child("Unsaved changes"))
        .child(format!("Save changes before you {target}?"))
        .child(div().flex().gap(px(8.)).justify_end().children(buttons));
    Some(
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::rgba(0x00000088))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(dialog)
            .into_any_element(),
    )
}
