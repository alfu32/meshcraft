//! A private JavaScriptCore worker process. No filesystem/network bindings enter JS.
use libloading::Library;
use std::{
    ffi::{CString, c_char, c_void},
    io::{self, BufRead, Write},
    sync::OnceLock,
};
type Ptr = *mut c_void;
type Eval = unsafe extern "C" fn(Ptr, Ptr, Ptr, Ptr, i32, *mut Ptr) -> Ptr;
struct Engine {
    _library: &'static Library,
    context: Ptr,
    release: unsafe extern "C" fn(Ptr),
    string: unsafe extern "C" fn(*const c_char) -> Ptr,
    string_release: unsafe extern "C" fn(Ptr),
    evaluate: Eval,
    to_string: unsafe extern "C" fn(Ptr, Ptr, *mut Ptr) -> Ptr,
    string_size: unsafe extern "C" fn(Ptr) -> usize,
    string_copy: unsafe extern "C" fn(Ptr, *mut c_char, usize) -> usize,
    group: Ptr,
    limit: unsafe extern "C" fn(Ptr, f64, unsafe extern "C" fn(Ptr, Ptr) -> bool, Ptr),
    clear: unsafe extern "C" fn(Ptr),
}
unsafe extern "C" fn terminate(_: Ptr, _: Ptr) -> bool {
    true
}
impl Engine {
    unsafe fn load() -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        let names = [
            "libjavascriptcoregtk-4.1.so.0",
            "libjavascriptcoregtk-6.0.so.1",
            "libjavascriptcoregtk-4.0.so.18",
        ];
        #[cfg(target_os = "macos")]
        let names = ["/System/Library/Frameworks/JavaScriptCore.framework/JavaScriptCore"];
        #[cfg(target_os = "windows")]
        let names = ["JavaScriptCore.dll"];
        // JavaScriptCore owns background GC/JIT threads. Its library must remain loaded
        // until process exit even after the last JSGlobalContext is released.
        static LIBRARY: OnceLock<Result<Library, String>> = OnceLock::new();
        let library = LIBRARY
            .get_or_init(|| {
                let mut errors = Vec::new();
                for name in names {
                    match unsafe { Library::new(name) } {
                        Ok(library) => return Ok(library),
                        Err(error) => errors.push(error.to_string()),
                    }
                }
                Err(format!(
                    "JavaScriptCore is unavailable. {}",
                    errors.join("; ")
                ))
            })
            .as_ref()
            .map_err(Clone::clone)?;
        // SAFETY: symbols below are the WebKit C API, kept alive by the owned library.
        unsafe {
            macro_rules! symbol {
                ($name:literal,$ty:ty) => {
                    *library
                        .get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|e| e.to_string())?
                };
            }
            let create = symbol!("JSGlobalContextCreate", unsafe extern "C" fn(Ptr) -> Ptr);
            let context = create(std::ptr::null_mut());
            if context.is_null() {
                return Err("could not create JavaScriptCore context".into());
            }
            let group = symbol!("JSContextGetGroup", unsafe extern "C" fn(Ptr) -> Ptr)(context);
            Ok(Self {
                context,
                group,
                release: symbol!("JSGlobalContextRelease", unsafe extern "C" fn(Ptr)),
                string: symbol!(
                    "JSStringCreateWithUTF8CString",
                    unsafe extern "C" fn(*const c_char) -> Ptr
                ),
                string_release: symbol!("JSStringRelease", unsafe extern "C" fn(Ptr)),
                evaluate: symbol!("JSEvaluateScript", Eval),
                to_string: symbol!(
                    "JSValueToStringCopy",
                    unsafe extern "C" fn(Ptr, Ptr, *mut Ptr) -> Ptr
                ),
                string_size: symbol!(
                    "JSStringGetMaximumUTF8CStringSize",
                    unsafe extern "C" fn(Ptr) -> usize
                ),
                string_copy: symbol!(
                    "JSStringGetUTF8CString",
                    unsafe extern "C" fn(Ptr, *mut c_char, usize) -> usize
                ),
                limit: symbol!(
                    "JSContextGroupSetExecutionTimeLimit",
                    unsafe extern "C" fn(Ptr, f64, unsafe extern "C" fn(Ptr, Ptr) -> bool, Ptr)
                ),
                clear: symbol!(
                    "JSContextGroupClearExecutionTimeLimit",
                    unsafe extern "C" fn(Ptr)
                ),
                _library: library,
            })
        }
    }
    fn evaluate(&self, input: &serde_json::Value) -> Result<serde_json::Value, String> {
        let code = CString::new(format!("{}({})", include_str!("script_sdk.js"), input))
            .map_err(|e| e.to_string())?;
        // SAFETY: the context is owned by this process/thread and all borrowed strings live through evaluation.
        unsafe {
            (self.limit)(self.group, 0.04, terminate, std::ptr::null_mut());
            let script = (self.string)(code.as_ptr());
            let mut exception = std::ptr::null_mut();
            let value = (self.evaluate)(
                self.context,
                script,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1,
                &mut exception,
            );
            (self.string_release)(script);
            (self.clear)(self.group);
            let failed = !exception.is_null() || value.is_null();
            let value = if exception.is_null() {
                value
            } else {
                exception
            };
            if value.is_null() {
                return Err("JavaScript execution limit reached".into());
            }
            let string = (self.to_string)(self.context, value, std::ptr::null_mut());
            if string.is_null() {
                return Err("could not serialize script result".into());
            }
            let size = (self.string_size)(string);
            if size > 32_000_000 {
                (self.string_release)(string);
                return Err("script output exceeds 32 MB".into());
            }
            let mut output = vec![0u8; size];
            let len = (self.string_copy)(string, output.as_mut_ptr().cast(), size);
            (self.string_release)(string);
            let text = String::from_utf8_lossy(&output[..len.saturating_sub(1)]);
            if failed {
                return Err(text.into_owned());
            }
            serde_json::from_str(&text).map_err(|e| format!("invalid script output: {e}"))
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        unsafe { (self.release)(self.context) }
    }
}
pub fn run_worker() {
    let input = io::stdin();
    let mut output = io::stdout().lock();
    for line in input.lock().lines() {
        let result = line.map_err(|e| e.to_string()).and_then(|s| {
            if s.len() > 32_000_000 {
                return Err("script input exceeds 32 MB".into());
            }
            let request = serde_json::from_str(&s).map_err(|e| e.to_string())?;
            // A fresh context bounds retained JS objects; instance state crosses frames as JSON.
            unsafe { Engine::load() }?.evaluate(&request)
        });
        let response = match result {
            Ok(v) => serde_json::json!({"ok":v}),
            Err(e) => serde_json::json!({"error":e}),
        };
        if writeln!(output, "{response}")
            .and_then(|_| output.flush())
            .is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meshcraft_core::document::{
        Document,
        scripts::{Drawable, ScriptDefinition, ScriptFrame, ScriptState},
    };
    use meshcraft_core::geometry::Vec3;
    fn request(create: &str, frame: &str, state: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"id":"test","source":{"onCreate":create,"onAnimationFrame":frame},"state":state,"geometry":[{"kind":"authored"}],"hotspots":{},"time":1.,"deltaTime":0.033,"model":{"objects":[],"materials":[],"settings":{}}})
    }
    #[test]
    fn javascriptcore_helpers_state_hotspots_and_explode_current_output() {
        let engine = unsafe { Engine::load() }
            .expect("JavaScriptCore must be installed for native runtime qualification");
        let source = "const h=hotspot(new Point(this.width,0,0),'width'); this.frames=(this.frames||0)+1;return [this.geometry,cube(new Point(),new Point(h.position.x,2,1),'body',phong('#83bdea80')),h];";
        let create = "this.width=3;";
        let first = engine
            .evaluate(&request(
                create,
                source,
                serde_json::json!({"initializer":"","properties":{}}),
            ))
            .unwrap();
        assert_eq!(3, first["state"]["properties"]["width"]);
        assert_eq!(1, first["state"]["properties"]["frames"]);
        let mut second_request = request(create, source, first["state"].clone());
        second_request["hotspots"] = serde_json::json!({"width":{"x":6,"y":0,"z":0}});
        let second = engine.evaluate(&second_request).unwrap();
        assert_eq!(2, second["state"]["properties"]["frames"]);
        let items: Vec<Drawable> = serde_json::from_value(second["items"].clone()).unwrap();
        let state: ScriptState = serde_json::from_value(second["state"].clone()).unwrap();
        let mut document = Document::new();
        let path = document
            .add_scripted_object(
                "test",
                ScriptDefinition {
                    on_create: create.into(),
                    on_animation_frame: source.into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let definition = document.resolve_occurrence(&path).unwrap().definition.id;
        document
            .put_hotspot(
                definition,
                meshcraft_core::document::hotspots::Hotspot {
                    id: "authored".into(),
                    position: Vec3::ZERO,
                    ..Default::default()
                },
            )
            .unwrap();
        let frame = ScriptFrame::build(
            &document
                .resolve_occurrence(&path)
                .unwrap()
                .definition
                .geometry,
            items,
            state,
        )
        .unwrap();
        assert_eq!(12, frame.geometry.triangles.len());
        assert_eq!(6., frame.hotspots[0].position.x);
        assert!(
            frame
                .geometry
                .triangles
                .values()
                .all(|f| f.color.alpha == 128)
        );
        document.install_script_frames(std::collections::BTreeMap::from([(path.clone(), frame)]));
        document
            .set_hotspot_target(&path, "width", Vec3::new(6., 0., 0.))
            .unwrap();
        document.persist_script_states();
        let copy = document
            .copy_root_occurrence(&path, Vec3::new(10., 0., 0.))
            .unwrap();
        assert_eq!(
            document.occurrence_edits(&path).unwrap().values,
            document.occurrence_edits(&copy).unwrap().values
        );
        let report = meshcraft_core::operations::objects::explode_occurrences(
            &mut document,
            &[path],
            &meshcraft_core::operations::objects::MeshOnlyExplodeAdapter,
        )
        .unwrap();
        assert_eq!(12, report.triangles);
        document.validate().unwrap();
        let exported = meshcraft_core::legacy::export_current_value(&document).unwrap();
        let reopened = meshcraft_core::legacy::import_current_value(&exported).unwrap();
        assert!(
            reopened
                .definitions()
                .any(|d| reopened.script(d.id).is_some())
        );
    }
    #[test]
    fn javascriptcore_limits_errors_and_all_primitive_helpers() {
        let engine = unsafe { Engine::load() }.unwrap();
        let result = engine.evaluate(&request(
            "",
            "while(true) {}",
            serde_json::json!({"initializer":"","properties":{}}),
        ));
        assert!(result.is_err());
        drop(engine);
        let engine = unsafe { Engine::load() }.unwrap();
        let result=engine.evaluate(&request("","return [circle(new Point(),2,'c'),sphere(new Point(),1,'s'),cone(new Point(),1,2,'cone'),polyline([new Point(),new Point(1,0,0)],'path'),face(new Point(),new Point(1,0,0),new Point(0,1,0),'f',gouraud('#ff0000'))];",serde_json::json!({"initializer":"","properties":{}}))).unwrap();
        assert!(result["items"].as_array().unwrap().len() > 100);
        assert!(
            engine
                .evaluate(&request(
                    "",
                    "return [line(new Point(),new Point(1,0,0),'x')];",
                    serde_json::json!({"initializer":"","properties":{}})
                ))
                .is_ok()
        );
    }
}

#[cfg(test)]
mod unified_tests {
    use super::*;
    #[test]
    fn unified_source_shares_helpers_between_hooks_and_queries_a_frozen_model() {
        let engine = unsafe { Engine::load() }.unwrap();
        let module = r##"
const tint=phong('#ffaa4488');
function dimensions() { return new Point(3, 2, 1); }
function onCreate(instance, model) { this.name=model.getObject('source').name; this.created=(this.created||0)+1; }
function onAnimationFrame(instance, model) {
  if(!Object.isFrozen(model.objects)) throw Error('model query is mutable');
  return [this.geometry,cube(new Point(),dimensions(),'body',tint),hotspot(new Point(3,0,0),'width')];
}
"##;
        let mut request = serde_json::json!({"id":"test","source":{"source":module},"state":{"initializer":"","properties":{}},"geometry":[],"hotspots":{},"time":0,"deltaTime":0.033,"model":{"objects":[{"id":"source","name":"Source Object"}],"materials":[],"settings":{}}});
        let first = engine.evaluate(&request).unwrap();
        assert_eq!("Source Object", first["state"]["properties"]["name"]);
        assert_eq!(25, first["items"].as_array().unwrap().len());
        request["state"] = first["state"].clone();
        let second = engine.evaluate(&request).unwrap();
        assert_eq!(1, second["state"]["properties"]["created"]);
        request["source"]["source"] = serde_json::Value::String("const helper=1;".into());
        assert!(
            engine
                .evaluate(&request)
                .unwrap_err()
                .contains("onAnimationFrame")
        );
    }
}
