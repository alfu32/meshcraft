//! Home screen: new projects by unit scale, Open, Import, recent files and samples.
use super::*;
use crate::app::{DOCUMENT_PRESETS, SAMPLE_PROJECTS, recent_project_display_name};
use std::path::PathBuf;
use std::time::SystemTime;

const SIDEBAR: u32 = 0x1d5586;
const SIDEBAR_ACTIVE: u32 = 0x2f6c9f;
const SIDEBAR_RULE: u32 = 0x4a7ba5;
const SURFACE: u32 = 0x1f232b;
const CARD: u32 = 0x272c35;
const CARD_HOVER: u32 = 0x313844;
const CARD_RULE: u32 = 0x3d444f;
const CARD_SELECTED: u32 = 0x1b3350;
const ACCENT: u32 = 0x4098ee;
const MUTED: u32 = 0xa2abb6;
const CONTENT_PADDING: f32 = 40.;
const CARD_GAP: f32 = 14.;
const SECTION_TITLE: f32 = 34.;
const SAMPLE_WIDTH: f32 = 222.;
const SAMPLE_HEIGHT: f32 = 228.;
const THUMBNAIL_DELAY: Duration = Duration::from_millis(1500);

gpui::actions!(home, [NextItem, PreviousItem]);

/// Beats the component root's Tab focus traversal while Home is on screen.
pub(super) const KEY_CONTEXT: &str = "MeshcraftHome";

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("tab", NextItem, Some(KEY_CONTEXT)),
        gpui::KeyBinding::new("shift-tab", PreviousItem, Some(KEY_CONTEXT)),
    ]);
}

#[derive(Clone, Debug, PartialEq)]
enum Action {
    Preset(&'static str),
    Open,
    Import,
    Recent(String),
    Sample(&'static str),
}

struct Recent {
    path: String,
    name: String,
    modified: Option<String>,
    exists: bool,
    thumbnail: Option<Arc<RenderImage>>,
}

#[derive(Default)]
pub(super) struct Ui {
    scroll: gpui::ScrollHandle,
    selected: usize,
    was_open: bool,
    scroll_extent: f32,
    recent_paths: Vec<String>,
    recents: Vec<Recent>,
    samples: BTreeMap<&'static str, Arc<RenderImage>>,
    capture_key: Option<(String, u64, u64)>,
    capture_since: Option<Instant>,
}

/// Commands that remain within the Home screen's own file workflow. Anything else
/// (a tool or panel chosen from the menu bar or palette) needs the workspace.
pub(super) fn leaves_home(id: &str) -> bool {
    !(id.starts_with("file.")
        || id.starts_with("interchange.")
        || id.starts_with("export.")
        || matches!(id, "import.mesh" | "app.quit" | "help.about"))
}

/// Human-readable file age for the Recent list.
fn age_text(age: Duration) -> String {
    let plural =
        |value: u64, unit: &str| format!("{value} {unit}{} ago", if value == 1 { "" } else { "s" });
    match age.as_secs() {
        0..60 => "Just now".to_owned(),
        s @ 60..3600 => plural(s / 60, "minute"),
        s @ 3600..86_400 => plural(s / 3600, "hour"),
        s @ 86_400..2_592_000 => plural(s / 86_400, "day"),
        s @ 2_592_000..31_536_000 => plural(s / 2_592_000, "month"),
        s => plural(s / 31_536_000, "year"),
    }
}

fn actions(shell: &MeshcraftShell) -> Vec<Action> {
    DOCUMENT_PRESETS
        .iter()
        .map(|preset| Action::Preset(preset.id))
        .chain([Action::Open, Action::Import])
        .chain(
            shell
                .home
                .recents
                .iter()
                .filter(|recent| recent.exists)
                .map(|recent| Action::Recent(recent.path.clone())),
        )
        .chain(
            SAMPLE_PROJECTS
                .iter()
                .map(|sample| Action::Sample(sample.id)),
        )
        .collect()
}

fn document_name(shell: &MeshcraftShell) -> String {
    shell
        .session
        .current_project_path()
        .map_or("Untitled".to_owned(), |path| {
            recent_project_display_name(&path).to_owned()
        })
}

fn activate(shell: &mut MeshcraftShell, action: &Action, cx: &mut Context<MeshcraftShell>) {
    let (id, key, value) = match action {
        Action::Preset(preset) => ("file.new", "preset", json!(preset)),
        Action::Open => ("file.open", "", json!(null)),
        Action::Import => ("import.mesh", "new_document", json!(true)),
        Action::Recent(path) => ("file.open", "path", json!(path)),
        Action::Sample(sample) => ("file.open_sample", "sample", json!(sample)),
    };
    let mut payload = CommandPayload::default();
    if !key.is_empty() {
        payload.values.insert(key.to_owned(), value);
    }
    shell.dispatch_payload(id, &payload);
    shell.refresh_scene();
    cx.notify();
}

/// Tab/arrows choose, Enter runs, Escape returns. Returns true when consumed.
pub(super) fn key_down(
    shell: &mut MeshcraftShell,
    event: &gpui::KeyDownEvent,
    cx: &mut Context<MeshcraftShell>,
) -> bool {
    let modifiers = event.keystroke.modifiers;
    if modifiers.control || modifiers.alt || modifiers.platform {
        return false;
    }
    match event.keystroke.key.as_str() {
        "escape" => shell.session.native_set_home_open(false),
        "enter" => {
            if let Some(action) = actions(shell).get(shell.home.selected) {
                activate(shell, action, cx);
            }
        }
        "right" | "down" => move_selection(shell, false),
        "left" | "up" => move_selection(shell, true),
        // Bare letters are tool hotkeys, which belong to the hidden workspace.
        _ => {}
    }
    true
}

pub(super) fn move_selection(shell: &mut MeshcraftShell, back: bool) {
    let actions = actions(shell);
    let Some(last) = actions.len().checked_sub(1) else {
        return;
    };
    shell.home.selected = match (back, shell.home.selected) {
        (true, 0) => last,
        (true, index) => index - 1,
        (false, index) if index >= last => 0,
        (false, index) => index + 1,
    };
    let section = match &actions[shell.home.selected] {
        Action::Preset(_) | Action::Open | Action::Import => 0,
        Action::Recent(_) => 1,
        Action::Sample(_) => 2,
    };
    reveal(shell, section + 1, &actions);
}

/// Bring the selected item into view; sample cards may sit several rows into a section.
fn reveal(shell: &MeshcraftShell, section: usize, actions: &[Action]) {
    let scroll = &shell.home.scroll;
    let view = scroll.bounds();
    let Some(sample) = actions
        .get(shell.home.selected)
        .and_then(|action| match action {
            Action::Sample(id) => SAMPLE_PROJECTS.iter().position(|sample| sample.id == *id),
            _ => None,
        })
    else {
        scroll.scroll_to_item(section);
        return;
    };
    let Some(top) = scroll.bounds_for_item(section).map(|bounds| bounds.top()) else {
        return;
    };
    let usable = f32::from(view.size.width) - 2. * CONTENT_PADDING;
    let columns = ((usable + CARD_GAP) / (SAMPLE_WIDTH + CARD_GAP))
        .floor()
        .max(1.);
    let row = (sample as f32 / columns).floor();
    let card_top = top - view.top() + px(SECTION_TITLE + row * (SAMPLE_HEIGHT + CARD_GAP));
    let mut offset = scroll.offset();
    if card_top + offset.y < px(0.) {
        offset.y = -card_top;
    } else if card_top + px(SAMPLE_HEIGHT) + offset.y > view.size.height {
        offset.y = view.size.height - card_top - px(SAMPLE_HEIGHT);
    }
    scroll.set_offset(offset);
}

fn refresh_recents(shell: &mut MeshcraftShell, window: &mut Window) {
    for recent in shell.home.recents.drain(..) {
        if let Some(image) = recent.thumbnail {
            let _ = window.drop_image(image);
        }
    }
    let now = SystemTime::now();
    shell.home.recent_paths = shell.session.native_recent_projects().to_vec();
    shell.home.recents = shell
        .home
        .recent_paths
        .iter()
        .map(|path| {
            let metadata = std::fs::metadata(path).ok().filter(|meta| meta.is_file());
            Recent {
                path: path.clone(),
                name: recent_project_display_name(path).to_owned(),
                modified: metadata
                    .as_ref()
                    .and_then(|meta| meta.modified().ok())
                    .map(|time| age_text(now.duration_since(time).unwrap_or_default())),
                exists: metadata.is_some(),
                thumbnail: crate::app::recent_thumbnail_path(path)
                    .and_then(|file| std::fs::read(file).ok())
                    .and_then(|bytes| crate::assets::gpui_png(&bytes)),
            }
        })
        .collect();
    shell.home.selected = 0;
}

pub(super) fn prepare(shell: &mut MeshcraftShell, window: &mut Window) {
    let open = shell.session.native_home_open();
    if open
        && (!shell.home.was_open
            || shell.home.recent_paths != shell.session.native_recent_projects())
    {
        refresh_recents(shell, window);
        window.focus(&shell.focus);
    }
    if open && shell.home.samples.is_empty() {
        shell.home.samples = SAMPLE_PROJECTS
            .iter()
            .filter_map(|sample| Some((sample.id, crate::assets::gpui_png(sample.thumbnail)?)))
            .collect();
    }
    // The scrollbar reads the extent measured by the previous paint.
    if open {
        let extent = f32::from(shell.home.scroll.max_offset().height);
        if !shell.home.was_open || extent != shell.home.scroll_extent {
            shell.home.scroll_extent = extent;
            window.request_animation_frame();
        }
    }
    shell.home.was_open = open;
}

/// Keep a Recent preview of the saved project once its view has settled.
pub(super) fn tick(shell: &mut MeshcraftShell) {
    if shell.session.native_home_open() || std::env::var_os("MESHCRAFT_DISABLE_AUTOSAVE").is_some()
    {
        return;
    }
    let Some(path) = shell.session.current_project_path() else {
        shell.home.capture_key = None;
        return;
    };
    let key = (
        path,
        shell.session.document_generation(),
        shell.session.document_revision(),
    );
    if shell.home.capture_key.as_ref() != Some(&key) {
        shell.home.capture_key = Some(key);
        shell.home.capture_since = Some(Instant::now());
        return;
    }
    if !shell
        .home
        .capture_since
        .is_some_and(|since| since.elapsed() >= THUMBNAIL_DELAY)
    {
        return;
    }
    shell.home.capture_since = None;
    let target = crate::app::recent_thumbnail_path(&key.0);
    if let (Some(frame), Some(target)) = (shell.bridge.latest_frame(), target) {
        let image = frame.image;
        let _ = std::thread::Builder::new()
            .name("recent-thumbnail".into())
            .spawn(move || write_thumbnail(&image, &target));
    }
}

/// Best effort: a missing preview only means the Recent row shows a generic icon.
fn write_thumbnail(frame: &RenderImage, target: &PathBuf) -> Option<()> {
    const WIDTH: u32 = 192;
    const HEIGHT: u32 = 120;
    let size = frame.size(0);
    let (width, height) = (u32::from(size.width), u32::from(size.height));
    let mut pixels = frame.as_bytes(0)?.to_vec();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let image = image::RgbaImage::from_raw(width, height, pixels)?;
    let crop_width = width.min(height * WIDTH / HEIGHT).max(1);
    let crop_height = height.min(width * HEIGHT / WIDTH).max(1);
    let crop = image::imageops::crop_imm(
        &image,
        (width - crop_width) / 2,
        (height - crop_height) / 2,
        crop_width,
        crop_height,
    )
    .to_image();
    let thumbnail = image::imageops::thumbnail(&crop, WIDTH, HEIGHT);
    std::fs::create_dir_all(target.parent()?).ok()?;
    let partial = target.with_extension("png.partial");
    thumbnail
        .save_with_format(&partial, image::ImageFormat::Png)
        .ok()?;
    std::fs::rename(partial, target).ok()
}

fn thumbnail_image(image: &Arc<RenderImage>) -> gpui::Img {
    gpui::img(gpui::ImageSource::Render(image.clone()))
}

fn section_title(title: &'static str) -> gpui::Div {
    div()
        .mb(px(10.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(title)
}

fn side_item(
    id: &'static str,
    label: impl Into<SharedString>,
    shortcut: Option<&'static str>,
    active: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(SharedString::from(format!("home/side/{id}")))
        .mx(px(10.))
        .px(px(12.))
        .h(px(36.))
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .rounded(px(6.))
        .text_sm()
        .text_color(rgb(0xffffff))
        .cursor_pointer()
        .when(active, |item| {
            item.bg(rgb(SIDEBAR_ACTIVE))
                .font_weight(gpui::FontWeight::SEMIBOLD)
        })
        .hover(|item| item.bg(rgb(SIDEBAR_ACTIVE)))
        .child(div().truncate().child(label.into()))
        .children(shortcut.map(|shortcut| {
            div()
                .flex_none()
                .text_xs()
                .text_color(rgb(0xd5e3ef))
                .child(shortcut)
        }))
}

fn rule() -> gpui::Div {
    div()
        .mx(px(22.))
        .my(px(10.))
        .h(px(1.))
        .flex_none()
        .bg(rgb(SIDEBAR_RULE))
}

fn sidebar(shell: &MeshcraftShell, cx: &mut Context<MeshcraftShell>) -> gpui::Div {
    let command = |id: &'static str,
                   label: &'static str,
                   shortcut: Option<&'static str>,
                   cx: &mut Context<MeshcraftShell>| {
        side_item(id, label, shortcut, false).on_click(cx.listener(move |shell, _, _, cx| {
            let dispatch = shell
                .session
                .dispatch_command(id, &CommandPayload::default());
            shell.apply_command_effects(id, &dispatch);
            // A cancelled file dialog keeps the choices on screen.
            if matches!(
                dispatch.result.completion,
                CommandCompletion::Pending { .. }
            ) || leaves_home(id)
            {
                shell.session.native_set_home_open(false);
            }
            shell.refresh_scene();
            cx.notify();
        }))
    };
    let action = |id: &'static str,
                  label: &'static str,
                  shortcut: Option<&'static str>,
                  action: Action,
                  cx: &mut Context<MeshcraftShell>| {
        side_item(id, label, shortcut, false)
            .on_click(cx.listener(move |shell, _, _, cx| activate(shell, &action, cx)))
    };
    div()
        .w(px(250.))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .pt(px(16.))
        .pb(px(10.))
        .gap(px(2.))
        .bg(rgb(SIDEBAR))
        .child(
            side_item(
                "back",
                format!("‹  Back to {}", document_name(shell)),
                Some("Esc"),
                false,
            )
            .mb(px(10.))
            .on_click(cx.listener(|shell, _, _, cx| {
                shell.session.native_set_home_open(false);
                cx.notify();
            })),
        )
        .child(
            side_item("start", "Start", None, true).on_click(cx.listener(|shell, _, _, cx| {
                shell.home.scroll.scroll_to_item(0);
                cx.notify();
            })),
        )
        .child(command("file.new", "New", None, cx))
        .child(action("open", "Open", Some("Ctrl+O"), Action::Open, cx))
        .child(action("import", "Import", None, Action::Import, cx))
        .child(rule())
        .child(command("file.save", "Save", Some("Ctrl+S"), cx))
        .child(command("file.save_as", "Save As", None, cx))
        .child(command("export.mesh", "Export", None, cx))
        .child(rule())
        .child(command("view.model_settings", "Model Settings", None, cx))
        .child(command(
            "view.panel.ui_settings.toggle",
            "Preferences",
            None,
            cx,
        ))
        .child(command("help.about", "About Meshcraft", None, cx))
}

fn card(
    id: String,
    action: Option<Action>,
    selected: bool,
    cx: &mut Context<MeshcraftShell>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(SharedString::from(id))
        .flex()
        .rounded(px(8.))
        .border_1()
        .border_color(rgb(if selected { ACCENT } else { CARD_RULE }))
        .bg(rgb(if selected { CARD_SELECTED } else { CARD }))
        .when_some(action, |card, action| {
            card.cursor_pointer()
                .hover(move |card| {
                    if selected {
                        card
                    } else {
                        card.bg(rgb(CARD_HOVER))
                    }
                })
                .on_click(cx.listener(move |shell, _, _, cx| activate(shell, &action, cx)))
        })
}

fn new_card(
    key: String,
    icon: gpui::AnyElement,
    title: &'static str,
    detail: &'static str,
    action: Action,
    selected: bool,
    cx: &mut Context<MeshcraftShell>,
) -> impl IntoElement {
    card(format!("home/new/{key}"), Some(action), selected, cx)
        .w(px(296.))
        .h(px(78.))
        .items_center()
        .gap(px(14.))
        .px(px(18.))
        .child(icon)
        .child(
            div()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .truncate()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(if selected { 0x8cc4f8 } else { 0xffffff }))
                        .child(title),
                )
                .child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(detail),
                ),
        )
}

fn atlas_icon(name: &str) -> gpui::AnyElement {
    crate::assets::gpui_icon(name).map_or_else(
        || div().size(px(32.)).into_any_element(),
        |icon| {
            thumbnail_image(&icon)
                .size(px(32.))
                .flex_none()
                .into_any_element()
        },
    )
}

fn recent_row(
    recent: &Recent,
    selected: bool,
    cx: &mut Context<MeshcraftShell>,
) -> impl IntoElement {
    let preview = recent.thumbnail.as_ref().map_or_else(
        || {
            div()
                .w(px(64.))
                .h(px(40.))
                .flex()
                .items_center()
                .justify_center()
                .child(atlas_icon("file"))
                .into_any_element()
        },
        |image| {
            thumbnail_image(image)
                .w(px(64.))
                .h(px(40.))
                .rounded(px(3.))
                .into_any_element()
        },
    );
    let row = card(
        format!("home/recent/{}", recent.path),
        recent.exists.then(|| Action::Recent(recent.path.clone())),
        selected,
        cx,
    );
    row.w_full()
        .h(px(58.))
        .items_center()
        .gap(px(14.))
        .px(px(14.))
        .when(!recent.exists, |row| row.opacity(0.55))
        .child(preview)
        .child(
            div()
                .w(px(220.))
                .flex_none()
                .truncate()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(recent.name.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(recent.path.clone()),
        )
        .child(
            div()
                .w(px(120.))
                .flex_none()
                .text_xs()
                .text_color(rgb(if recent.exists { MUTED } else { 0xe89a8c }))
                .child(recent.modified.clone().unwrap_or("File not found".into())),
        )
}

pub(super) fn render(shell: &MeshcraftShell, cx: &mut Context<MeshcraftShell>) -> gpui::AnyElement {
    let selected_action = actions(shell).get(shell.home.selected).cloned();
    let is_selected = |action: &Action| selected_action.as_ref() == Some(action);
    let presets = DOCUMENT_PRESETS.iter().map(|preset| {
        let action = Action::Preset(preset.id);
        let selected = is_selected(&action);
        new_card(
            preset.id.to_owned(),
            atlas_icon(preset.icon),
            preset.title,
            preset.detail,
            action,
            selected,
            cx,
        )
        .into_any_element()
    });
    let presets: Vec<_> = presets.collect();
    let open = new_card(
        "open".into(),
        atlas_icon("open"),
        "Open…",
        "A Meshcraft .octd or legacy .k3d project",
        Action::Open,
        is_selected(&Action::Open),
        cx,
    )
    .into_any_element();
    let import = new_card(
        "import".into(),
        gpui::svg()
            .path("icons/import.svg")
            .size(px(32.))
            .flex_none()
            .text_color(rgb(ACCENT))
            .into_any_element(),
        "Import…",
        "A mesh or IFC model into a new project",
        Action::Import,
        is_selected(&Action::Import),
        cx,
    )
    .into_any_element();
    let recents: Vec<_> = shell
        .home
        .recents
        .iter()
        .map(|recent| {
            recent_row(
                recent,
                is_selected(&Action::Recent(recent.path.clone())),
                cx,
            )
            .into_any_element()
        })
        .collect();
    let samples: Vec<_> = SAMPLE_PROJECTS
        .iter()
        .map(|sample| {
            let action = Action::Sample(sample.id);
            let selected = is_selected(&action);
            card(
                format!("home/sample/{}", sample.id),
                Some(action),
                selected,
                cx,
            )
            .w(px(SAMPLE_WIDTH))
            .h(px(SAMPLE_HEIGHT))
            .flex_col()
            .p(px(9.))
            .children(shell.home.samples.get(sample.id).map(|image| {
                thumbnail_image(image)
                    .w(px(204.))
                    .h(px(127.))
                    .flex_none()
                    .rounded(px(4.))
            }))
            .child(
                div()
                    .mt(px(9.))
                    .flex_none()
                    .truncate()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(sample.title),
            )
            .child(
                div()
                    .mt(px(4.))
                    .line_clamp(2)
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(sample.detail),
            )
            .into_any_element()
        })
        .collect();
    let empty_recents = recents.is_empty();
    let content =
        div()
            .id("home/content")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&shell.home.scroll)
            .px(px(CONTENT_PADDING))
            .py(px(30.))
            .child(
                div()
                    .mb(px(26.))
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Start"),
                    )
                    .child(
                        div()
                            .mt(px(6.))
                            .text_sm()
                            .text_color(rgb(0xcdd3da))
                            .child("Create a project, open a recent file, or explore a sample."),
                    ),
            )
            .child(
                div().mb(px(28.)).child(section_title("New")).child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(CARD_GAP))
                        .children(presets)
                        .child(open)
                        .child(import),
                ),
            )
            .child(
                div().mb(px(28.)).child(section_title("Recent")).child(
                    div().flex().flex_col().gap(px(8.)).children(recents).when(
                        empty_recents,
                        |list| {
                            list.child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child("Projects you open or save appear here."),
                            )
                        },
                    ),
                ),
            )
            .child(
                div()
                    .pb(px(10.))
                    .child(section_title("Samples"))
                    .child(div().flex().flex_wrap().gap(px(CARD_GAP)).children(samples)),
            );
    div()
        .flex_1()
        .min_h(px(0.))
        .flex()
        .bg(rgb(SURFACE))
        .text_color(rgb(0xf2f4f7))
        .child(sidebar(shell, cx))
        .child(
            div()
                .relative()
                .flex_1()
                .min_w(px(0.))
                .h_full()
                .child(content)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .child(
                            gpui_component::scroll::Scrollbar::vertical(&shell.home.scroll)
                                .scrollbar_show(gpui_component::scroll::ScrollbarShow::Always),
                        ),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_ages_read_naturally() {
        for (seconds, expected) in [
            (5, "Just now"),
            (60, "1 minute ago"),
            (7200, "2 hours ago"),
            (86_400, "1 day ago"),
            (5_184_000, "2 months ago"),
            (63_072_000, "2 years ago"),
        ] {
            assert_eq!(expected, age_text(Duration::from_secs(seconds)));
        }
    }

    #[test]
    fn only_workspace_commands_leave_home() {
        for id in [
            "tool.select",
            "view.model_settings",
            "view.panel.ui_settings.toggle",
        ] {
            assert!(leaves_home(id), "{id}");
        }
        for id in [
            "file.home",
            "file.save",
            "import.mesh",
            "export.mesh",
            "help.about",
        ] {
            assert!(!leaves_home(id), "{id}");
        }
    }
}
