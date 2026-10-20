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
        unsafe { Self::in_group(std::ptr::null_mut()) }
    }
    unsafe fn in_group(parent: Ptr) -> Result<Self, String> {
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
            let create = symbol!(
                "JSGlobalContextCreateInGroup",
                unsafe extern "C" fn(Ptr, Ptr) -> Ptr
            );
            let context = create(parent, std::ptr::null_mut());
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
    #[cfg(test)]
    fn evaluate(&self, input: &serde_json::Value) -> Result<serde_json::Value, String> {
        let code = CString::new(format!("{}({})", include_str!("script_sdk.js"), input))
            .map_err(|e| e.to_string())?;
        let script = unsafe { (self.string)(code.as_ptr()) };
        let result = self.execute(script);
        unsafe {
            (self.string_release)(script);
        }
        result
    }
    fn execute(&self, script: Ptr) -> Result<serde_json::Value, String> {
        // SAFETY: the context is owned by this thread; the caller retains the script string.
        unsafe {
            (self.limit)(self.group, 0.04, terminate, std::ptr::null_mut());
            let mut exception = std::ptr::null_mut();
            let value = (self.evaluate)(
                self.context,
                script,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1,
                &mut exception,
            );
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
/// Reuse the VM, but give every invocation a fresh global object. Only an immutable
/// JSON string crosses contexts; scripts cannot poison another instance's globals.
pub(super) struct Session {
    owner: Engine,
    model: Ptr,
    sdk: Ptr,
}
impl Session {
    pub(super) fn new(model: &serde_json::Value) -> Result<Self, String> {
        let owner = unsafe { Engine::load() }?;
        let json = CString::new(model.to_string()).map_err(|e| e.to_string())?;
        let model = unsafe { (owner.string)(json.as_ptr()) };
        let code = CString::new(format!(
            "{}(__meshcraftRequest)",
            include_str!("script_sdk.js")
        ))
        .map_err(|e| e.to_string())?;
        let sdk = unsafe { (owner.string)(code.as_ptr()) };
        Ok(Self { owner, model, sdk })
    }
    pub(super) fn evaluate(
        &self,
        request: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let engine = unsafe { Engine::in_group(self.owner.group) }?;
        // SAFETY: these are stable JavaScriptCore C APIs. The retained JSString lives
        // through evaluation, and its primitive value is rooted on the fresh global.
        unsafe {
            let global = *engine
                ._library
                .get::<unsafe extern "C" fn(Ptr) -> Ptr>(b"JSContextGetGlobalObject\0")
                .map_err(|e| e.to_string())?;
            let make = *engine
                ._library
                .get::<unsafe extern "C" fn(Ptr, Ptr) -> Ptr>(b"JSValueMakeString\0")
                .map_err(|e| e.to_string())?;
            let set = *engine
                ._library
                .get::<unsafe extern "C" fn(Ptr, Ptr, Ptr, Ptr, u32, *mut Ptr)>(
                    b"JSObjectSetProperty\0",
                )
                .map_err(|e| e.to_string())?;
            let parse = *engine
                ._library
                .get::<unsafe extern "C" fn(Ptr, Ptr) -> Ptr>(b"JSValueMakeFromJSONString\0")
                .map_err(|e| e.to_string())?;
            let name = (engine.string)(c"__meshcraftModelJson".as_ptr());
            let mut exception = std::ptr::null_mut();
            set(
                engine.context,
                global(engine.context),
                name,
                make(engine.context, self.model),
                // kJSPropertyAttributeReadOnly | DontEnum | DontDelete.
                14,
                &mut exception,
            );
            (engine.string_release)(name);
            if !exception.is_null() {
                return Err("could not install model snapshot".into());
            }
            let name = (engine.string)(c"__meshcraftRequest".as_ptr());
            let json = CString::new(request.to_string()).map_err(|e| e.to_string())?;
            let input = (engine.string)(json.as_ptr());
            let value = parse(engine.context, input);
            (engine.string_release)(input);
            if value.is_null() {
                (engine.string_release)(name);
                return Err("could not decode script input".into());
            }
            set(
                engine.context,
                global(engine.context),
                name,
                value,
                14,
                &mut exception,
            );
            (engine.string_release)(name);
            if !exception.is_null() {
                return Err("could not install script input".into());
            }
        }
        engine.execute(self.sdk)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            (self.owner.string_release)(self.model);
            (self.owner.string_release)(self.sdk);
        }
    }
}
pub fn run_worker() {
    let input = io::stdin();
    let mut output = io::stdout().lock();
    let mut model = serde_json::Value::Null;
    let mut session: Option<Session> = None;
    for line in input.lock().lines() {
        let result = line.map_err(|e| e.to_string()).and_then(|s| {
            if s.len() > 32_000_000 {
                return Err("script input exceeds 32 MB".into());
            }
            let request: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
            if let Some(snapshot) = request.get("prepareModel") {
                model = snapshot.clone();
                // A fresh VM per sweep also bounds retained JIT/GC objects.
                session = Some(Session::new(&model)?);
                return Ok(serde_json::Value::Null);
            }
            if session.is_none() {
                session = Some(Session::new(&model)?);
            }
            let result = session.as_ref().unwrap().evaluate(&request);
            if result.is_err() {
                session = None;
            }
            result
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

#[cfg(test)]
mod scaling_tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn shared_vm_keeps_globals_isolated_and_scene_queries_immutable() {
        let session = Session::new(&json!({"objects":[{"id":"source","name":"Frame","geometry":[1,2,3]}],"materials":[],"settings":{}})).unwrap();
        let mut request = json!({"id":"0","geometry":[],"hotspots":{},"state":{"initializer":"","properties":{}},"time":2.,"deltaTime":0.033,"source":{"source":r#"
function onCreate(){this.created=(this.created||0)+1;}
function onAnimationFrame(instance,model){
 if(globalThis.polluted || Object.prototype.polluted) throw Error('globals leaked');
 globalThis.polluted=true; Object.prototype.polluted=true;
 this.frames=(this.frames||0)+1;
 this.name=model.getObject('source').name;
 this.count=model.queryGeometry('source').length;
 if(!Object.isFrozen(model.objects[0].geometry))throw Error('mutable query');
 return cube(new Point(),new Point(3,2,1),'body');
}"#}});
        for _ in 0..3 {
            let result = session.evaluate(&request).unwrap();
            assert_eq!(24, result["items"].as_array().unwrap().len());
            assert_eq!("Frame", result["state"]["properties"]["name"]);
            assert_eq!(3, result["state"]["properties"]["count"]);
            assert_eq!(1, result["state"]["properties"]["created"]);
            request["state"] = result["state"].clone();
        }
        assert_eq!(3, request["state"]["properties"]["frames"]);
    }
    #[test]
    fn local_generators_do_not_parse_an_unused_scene_snapshot() {
        // A null scene cannot be queried, but generators that don't query it work.
        let session = Session::new(&Value::Null).unwrap();
        for i in 0..64 {
            let result = session.evaluate(&json!({"id":i.to_string(),"geometry":[],"hotspots":{},"time":0.,"deltaTime":0.,"source":{"source":"function onAnimationFrame(){return cube(new Point(),1,this.id);}"}})).unwrap();
            assert_eq!(24, result["items"].as_array().unwrap().len());
        }
    }
}

#[cfg(test)]
mod construction_tests {
    use super::*;
    use meshcraft_core::document::{
        GeometryBundle,
        scripts::{Drawable, ScriptFrame, ScriptState},
    };
    fn run(source: &str) -> serde_json::Value {
        let session = Session::new(&serde_json::Value::Null).unwrap();
        session.evaluate(&serde_json::json!({"id":"part","geometry":[],"hotspots":{},"time":0,"deltaTime":0,"source":{"source":format!("function onAnimationFrame(){{{source}}}")}})).unwrap()
    }
    #[test]
    fn script_transforms_points_and_nested_geometry_without_mutation() {
        let out = run(
            "const p=new Point(2,0,0,'p'); const q=translate(new Point(1,0,0))(rotate(new Point(1,0,0),new Point(0,1,0),90)(p));if(p.x!==2||Math.abs(q.x-2)>1e-9||Math.abs(q.z+1)>1e-9)throw Error('rotation');if(abs(sin(90)-1)>1e-9||norm(new Point(3,4,0))!==5)throw Error('math');const c=cuboid(new Point(),new Point(2,3,4),'body');return [c,withIds(scale(new Point(-1,1,1))(c),'mirror'),rectangle(new Point(),new Point(2,0,3),'r'),circle(new Point(),2),polyline([new Point(),new Point(2,0,0)])];",
        );
        let items: Vec<Drawable> = serde_json::from_value(out["items"].clone()).unwrap();
        let frame =
            ScriptFrame::build(&GeometryBundle::default(), items, ScriptState::default()).unwrap();
        assert_eq!(26, frame.geometry.triangles.len());
        assert_eq!(41, frame.geometry.segments.len());
    }
    #[test]
    fn script_builds_concave_polygons_and_extrusions_and_seeded_random_values() {
        let out = run(
            "const p=[new Point(),new Point(3,0,0),new Point(3,0,3),new Point(1,0,1),new Point(0,0,3)];if(JSON.stringify(rands(0,1,5,8))!==JSON.stringify(rands(0,1,5,8)))throw Error('seed');if(lookup(0.5,[[0,0],[1,10]])!==5)throw Error('lookup');return [polygon(p,'poly'),linear_extrude(2,p,'extrude'),color([1,0,0,0.5])(cylinder(new Point(),1,2,'cylinder',undefined,12))];",
        );
        let items: Vec<Drawable> = serde_json::from_value(out["items"].clone()).unwrap();
        let frame =
            ScriptFrame::build(&GeometryBundle::default(), items, ScriptState::default()).unwrap();
        assert_eq!(67, frame.geometry.triangles.len());
        assert!(
            frame
                .geometry
                .triangles
                .values()
                .any(|f| f.color.alpha == 128)
        );
    }
}
