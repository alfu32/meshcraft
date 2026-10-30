//! Searchable command shelf and catalog-backed ribbon pickers.
use super::*;
use gpui_component::select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState};

#[derive(Clone)]
struct Choice {
    id: String,
    label: String,
}
impl SelectItem for Choice {
    type Value = String;
    fn title(&self) -> SharedString {
        self.label.clone().into()
    }
    fn value(&self) -> &String {
        &self.id
    }
}
pub(super) struct Ui {
    pub bounds: Arc<Mutex<BTreeMap<String, gpui::Bounds<gpui::Pixels>>>>,
    selected_materials: Vec<String>,
    pub width: f32,
    pub resizing: bool,
    resize_start: Option<(f32, f32)>,
    pub query: String,
    search_focused: bool,
    search: Option<Entity<InputState>>,
    search_subscription: Option<gpui::Subscription>,
    key: String,
    material: Option<Entity<SelectState<SearchableVec<Choice>>>>,
    object: Option<Entity<SelectState<SearchableVec<Choice>>>>,
    subscriptions: Vec<gpui::Subscription>,
    material_placeholder: String,
}
impl Ui {
    pub fn new(width: f32) -> Self {
        Self {
            bounds: Arc::default(),
            selected_materials: vec![],
            width,
            resizing: false,
            resize_start: None,
            query: String::new(),
            search_focused: false,
            search: None,
            search_subscription: None,
            key: String::new(),
            material: None,
            object: None,
            subscriptions: vec![],
            material_placeholder: String::new(),
        }
    }
}
pub(super) fn prepare(
    shell: &mut MeshcraftShell,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    let saved = shell.session.native_ui_preferences().left_panel_width;
    if !shell.workspace_controls.resizing {
        let maximum = (f32::from(window.viewport_size().width) - layout::RIGHT_PANEL_WIDTH - 260.)
            .clamp(180., 600.);
        shell.workspace_controls.width = saved.clamp(180., maximum);
    }
    if shell.workspace_controls.search.is_none() {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search tools…"));
        shell.workspace_controls.search_subscription = Some(cx.subscribe_in(
            &input,
            window,
            |this, input, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Focus => this.workspace_controls.search_focused = true,
                    InputEvent::Blur => this.workspace_controls.search_focused = false,
                    InputEvent::Change => {
                        this.workspace_controls.query = input.read(cx).value().to_string()
                    }
                    InputEvent::PressEnter { .. } => {
                        let query = this.workspace_controls.query.clone();
                        let commands = crate::MeshcraftApp::command_catalog();
                        if let Some(command) = commands.iter().find(|command| {
                            crate::functional_ui::placement(command.id.as_str()).is_some()
                                && matches(command, &query)
                                && command_enabled(&this.session, command)
                        }) {
                            window.focus(&this.focus);
                            this.dispatch_with_clipboard(command.id.as_str(), cx);
                        }
                    }
                }
                cx.notify();
            },
        ));
        shell.workspace_controls.search = Some(input);
    }
    let key = format!(
        "{}:{}:{}:{:?}",
        shell.session.document_generation(),
        shell.cached_document.revision(),
        shell.session.native_selection_revision(),
        shell.materials.selected
    );
    if shell.workspace_controls.key == key {
        return;
    }
    shell.workspace_controls.key = key;
    shell.workspace_controls.subscriptions.clear();
    let selected = shell.session.native_selection_materials();
    shell.workspace_controls.selected_materials = selected.iter().cloned().collect();
    let choices = shell
        .cached_document
        .materials()
        .into_iter()
        .map(|m| Choice {
            id: m.id,
            label: m.name,
        })
        .collect::<Vec<_>>();
    let current = if shell.session.native_selection_targets().is_empty() {
        shell.materials.selected.as_deref()
    } else if selected.len() == 1 {
        selected.first().map(String::as_str)
    } else {
        None
    };
    let index = current
        .and_then(|id| choices.iter().position(|m| m.id == id))
        .map(|i| gpui_component::IndexPath::default().row(i));
    shell.workspace_controls.material_placeholder = if selected.len() > 1 {
        "Multiple materials"
    } else {
        "Choose material"
    }
    .into();
    let picker = cx.new(|cx| {
        SelectState::new(SearchableVec::new(choices), index, window, cx).searchable(true)
    });
    shell.workspace_controls.subscriptions.push(cx.subscribe_in(
        &picker,
        window,
        |this, _, event: &SelectEvent<SearchableVec<Choice>>, window, cx| {
            if let SelectEvent::Confirm(Some(id)) = event {
                this.materials.selected = Some(id.clone());
                let result = this.session.choose_native_material(id).and_then(|()| {
                    if this.session.native_selection_targets().is_empty() {
                        Ok(())
                    } else {
                        this.session.assign_native_material(id)
                    }
                });
                if let Err(error) = result {
                    this.session.set_native_status(error);
                }
                window.focus(&this.focus);
                this.refresh_scene();
                cx.notify();
            }
        },
    ));
    shell.workspace_controls.material = Some(picker);
    let doc = &shell.cached_document;
    let scene = doc
        .scene_container_occurrence()
        .and_then(|p| doc.resolve_occurrence(&p).map(|r| r.definition.id));
    let choices = doc
        .definitions()
        .filter(|d| {
            d.id != Document::SCENE_ROOT_DEFINITION_ID
                && Some(d.id) != scene
                && !doc.is_frame_definition(d.id)
        })
        .map(|d| Choice {
            id: d.id.0.to_string(),
            label: d.name.clone(),
        })
        .collect::<Vec<_>>();
    let picker = cx
        .new(|cx| SelectState::new(SearchableVec::new(choices), None, window, cx).searchable(true));
    shell.workspace_controls.subscriptions.push(cx.subscribe_in(
        &picker,
        window,
        |this, _, event: &SelectEvent<SearchableVec<Choice>>, window, cx| {
            if let SelectEvent::Confirm(Some(id)) = event
                && let Ok(id) = id.parse::<u64>()
            {
                this.session
                    .select_native_prototype(meshcraft_core::document::DefinitionId(id));
                let mut payload = CommandPayload::default();
                payload.values.insert("definition_id".into(), json!(id));
                this.dispatch_payload("object.place_selected_definition", &payload);
                window.focus(&this.focus);
                cx.notify();
            }
        },
    ));
    shell.workspace_controls.object = Some(picker);
}
pub(super) fn capture_key(
    shell: &mut MeshcraftShell,
    event: &gpui::KeyDownEvent,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) -> bool {
    if !shell.workspace_controls.search_focused {
        return false;
    }
    let key = &event.keystroke.key;
    if key == "escape" {
        shell.workspace_controls.query.clear();
        if let Some(input) = shell.workspace_controls.search.clone() {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        window.focus(&shell.focus);
    } else if key == "p"
        && event.keystroke.modifiers.shift
        && (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
    {
        shell.toggle_command_palette();
        window.focus(&shell.focus);
    } else {
        return false;
    }
    cx.notify();
    true
}
pub(super) fn matches(command: &CommandDescriptor, query: &str) -> bool {
    let text = format!(
        "{} {} {} {} {}",
        command.presentation.palette_path,
        command.category,
        command.label,
        command.id.as_str(),
        command.keywords.join(" ")
    )
    .to_lowercase();
    query
        .split_whitespace()
        .all(|word| text.contains(&word.to_lowercase()))
}
pub(super) fn bounds_probe(shell: &MeshcraftShell, key: String) -> gpui::AnyElement {
    let store = shell.workspace_controls.bounds.clone();
    canvas(
        move |bounds, _, _| {
            if let Ok(mut map) = store.lock() {
                map.insert(key.clone(), bounds);
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .left_0()
    .top_0()
    .size_full()
    .into_any_element()
}
pub(super) fn status(shell: &MeshcraftShell) -> Value {
    let p = shell.session.native_ui_preferences();
    let controls=shell.workspace_controls.bounds.lock().map(|map|map.iter().filter(|(key,_)| {
        (p.left_panel_visible && key.starts_with("left/")) || (p.ribbon_visible && (key.starts_with("ribbon/") || (shell.active_ribbon_tab=="Home" && key.starts_with("home/")) || (shell.active_ribbon_tab=="Materials" && key.starts_with("materials/"))))
    }).map(|(key,b)|(key.clone(),json!({"x":f32::from(b.origin.x),"y":f32::from(b.origin.y),"width":f32::from(b.size.width),"height":f32::from(b.size.height)}))).collect::<BTreeMap<_,_>>()).unwrap_or_default();
    json!({"ribbonTab":shell.active_ribbon_tab,"ribbonTabs":crate::functional_ui::RIBBON_TABS,"leftPanelWidth":shell.workspace_controls.width,"leftPanelQuery":shell.workspace_controls.query,"selectionMaterials":shell.workspace_controls.selected_materials,"controls":controls,"coordinateSpace":"window logical pixels"})
}
pub(super) fn search(shell: &MeshcraftShell) -> gpui::AnyElement {
    div()
        .relative()
        .w_full()
        .child(
            Input::new(
                shell
                    .workspace_controls
                    .search
                    .as_ref()
                    .expect("prepared search"),
            )
            .small()
            .w_full(),
        )
        .child(bounds_probe(shell, "left/search".into()))
        .into_any_element()
}
pub(super) fn picker(
    shell: &MeshcraftShell,
    object: bool,
    namespace: &'static str,
) -> gpui::AnyElement {
    let state = if object {
        shell.workspace_controls.object.as_ref()
    } else {
        shell.workspace_controls.material.as_ref()
    };
    div()
        .id(SharedString::from(format!(
            "{namespace}/{}",
            if object { "object" } else { "material" }
        )))
        .w(px(210.))
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(4.))
        .px(px(7.))
        .py(px(4.))
        .child(div().text_xs().text_color(rgb(0xa5bbce)).child(if object {
            "Insert Object"
        } else {
            "Selection Material"
        }))
        .when_some(state, |panel, state| {
            panel.child(
                div()
                    .relative()
                    .w_full()
                    .child(Select::new(state).small().w_full().placeholder(if object {
                        "Choose prototype…".to_owned()
                    } else {
                        shell.workspace_controls.material_placeholder.clone()
                    }))
                    .child(bounds_probe(shell, namespace.into())),
            )
        })
        .into_any_element()
}
pub(super) fn start_resize(shell: &mut MeshcraftShell, x: f32) {
    shell.workspace_controls.resizing = true;
    shell.workspace_controls.resize_start = Some((x, shell.workspace_controls.width));
}
pub(super) fn resize_move(
    shell: &mut MeshcraftShell,
    event: &gpui::MouseMoveEvent,
    window: &mut Window,
    cx: &mut Context<MeshcraftShell>,
) {
    if let Some((start, width)) = shell.workspace_controls.resize_start {
        let maximum = (f32::from(window.viewport_size().width) - layout::RIGHT_PANEL_WIDTH - 260.)
            .clamp(180., 600.);
        shell.workspace_controls.width =
            (width + f32::from(event.position.x) - start).clamp(180., maximum);
        if event.pressed_button != Some(gpui::MouseButton::Left) {
            finish_resize(shell, cx);
        }
        cx.notify();
    }
}
pub(super) fn finish_resize(shell: &mut MeshcraftShell, cx: &mut Context<MeshcraftShell>) {
    if !shell.workspace_controls.resizing {
        return;
    }
    shell.workspace_controls.resizing = false;
    shell.workspace_controls.resize_start = None;
    shell.update_ui_preference("left_panel_width", json!(shell.workspace_controls.width));
    cx.notify();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shelf_search_matches_category_name_id_keywords_and_each_query_word() {
        let catalog = MeshcraftApp::command_catalog();
        let wall = catalog
            .iter()
            .find(|c| c.id.as_str() == "tool.builtin.arch_wall")
            .unwrap();
        assert!(matches(wall, "CIVIL wall"));
        assert!(matches(wall, "archi"));
        assert!(matches(wall, "tool.builtin.arch_wall"));
        assert!(!matches(wall, "civil unknown-tool"));
        assert!(matches(wall, ""));
    }
}
