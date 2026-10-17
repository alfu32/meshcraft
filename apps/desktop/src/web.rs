use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::MeshcraftApp;

#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("meshcraft_canvas"))
        .ok_or_else(|| JsValue::from_str("missing #meshcraft_canvas element"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(|creation_context| Ok(Box::new(MeshcraftApp::new(creation_context)))),
        )
        .await
}
