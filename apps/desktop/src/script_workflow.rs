use super::*;
use meshcraft_core::document::scripts::{ScriptDefinition, ScriptFrame};
impl MeshcraftApp {
    pub fn save_native_script(
        &mut self,
        id: meshcraft_core::document::DefinitionId,
        script: ScriptDefinition,
    ) -> Result<(), String> {
        self.save_native_script_with_declarations(id, script, None)?;
        self.status = "Saved prototype script.".into();
        Ok(())
    }
    pub fn save_native_script_with_declarations(
        &mut self,
        id: meshcraft_core::document::DefinitionId,
        script: ScriptDefinition,
        declarations: Option<Vec<meshcraft_core::document::scripts::parameters::Declaration>>,
    ) -> Result<(), String> {
        self.commit_global_document_edit("Save object script", |document| {
            document.set_script(id, script)?;
            if let Some(declarations) = declarations {
                document.set_script_parameter_declarations(id, declarations)?;
            }
            Ok(())
        })?;
        Ok(())
    }
    pub fn create_native_scripted_object(
        &mut self,
        script: ScriptDefinition,
    ) -> Result<meshcraft_core::document::DefinitionId, String> {
        let path = self.commit_global_document_edit("Create scripted object", |document| {
            document.add_scripted_object("Scripted Object", script)
        })?;
        let id = self
            .document
            .resolve_occurrence(&path)
            .ok_or("missing object")?
            .definition
            .id;
        self.object_browser_selection = Some(id);
        self.set_selection([path]);
        self.panel_focus = Some("object".into());
        self.status = "Created scripted object.".into();
        Ok(id)
    }
    pub fn install_native_script_frames(&mut self, frames: BTreeMap<OccurrenceRef, ScriptFrame>) {
        self.document.install_script_frames(frames);
    }
    pub fn native_visual_revision(&self) -> u64 {
        self.document.visual_revision()
    }
}
