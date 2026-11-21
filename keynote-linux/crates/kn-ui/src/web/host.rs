//! The `postMessage` bridge to a same-origin parent page (`?host=parent`), for pages that embed
//! Keydeck, Values or Letters in an `<iframe>` and store their documents (for example a file
//! server or groupware app).
//!
//! Each message is a plain JS object with a `type` prefixed by the app name (`keydeck`, `values`
//! or `letters`):
//! - `<app>:ready` (to parent): `{ version }`, sent once `WebRunner` is up and listening.
//! - `<app>:failed` (to parent): `{ error }`, sent if startup fails (e.g. no WebGPU / WebGL2).
//! - `<app>:open` (from parent): `{ name, bytes }` (`ArrayBuffer` or `Uint8Array`).
//! - `<app>:save` (to parent): `{ name, saveAs, bytes }` (transferred `ArrayBuffer`).
//! - `<app>:dirty` (to parent): `{ dirty }` whenever unsaved changes appear or clear.

/// Maximum length accepted from `?author=`.
pub const MAX_AUTHOR_CHARS: usize = 200;

/// Format a namespaced host message type, e.g. `"keydeck:save"`.
pub fn message_type(prefix: &str, kind: &str) -> String {
    format!("{prefix}:{kind}")
}

/// Sanitize a host-supplied `?author=` query parameter (trim whitespace, clamp length).
pub fn sanitize_author(raw: &str) -> Option<String> {
    let cleaned: String = raw.trim().chars().take(MAX_AUTHOR_CHARS).collect();
    (!cleaned.is_empty()).then_some(cleaned)
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::Cell;
    use std::rc::Rc;

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast as _, JsValue};

    use super::message_type;
    use crate::web::files::WebEvent;

    #[derive(Clone)]
    pub struct Bridge {
        window: web_sys::Window,
        parent: web_sys::Window,
        origin: String,
        prefix: &'static str,
        default_name: &'static str,
        reported_dirty: Rc<Cell<Option<bool>>>,
    }

    impl Bridge {
        /// Connect to a same-origin parent window. Returns `None` if not framed, if the origin is
        /// opaque (`"null"`), or if the parent is cross-origin.
        pub fn new(prefix: &'static str, default_name: &'static str) -> Option<Self> {
            let window = web_sys::window()?;
            let parent = window.parent().ok().flatten()?;
            if js_sys::Object::is(&parent, &window) {
                return None;
            }
            let origin = window.location().origin().ok()?;
            if origin == "null" {
                return None;
            }
            if parent.location().origin().ok()? != origin {
                return None;
            }
            Some(Self {
                window,
                parent,
                origin,
                prefix,
                default_name,
                reported_dirty: Rc::new(Cell::new(None)),
            })
        }

        /// Listen for `<prefix>:open` messages from the same-origin parent window.
        pub fn listen(&self, ctx: egui::Context) {
            let parent = self.parent.clone();
            let origin = self.origin.clone();
            let open_type = message_type(self.prefix, "open");
            let default_name = self.default_name;
            let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
                if event.origin() != origin || !event.source().is_some_and(|s| js_sys::Object::is(&s, &parent)) {
                    return;
                }
                let data = event.data();
                if string(&data, "type").as_deref() != Some(open_type.as_str()) {
                    return;
                }
                let name = string(&data, "name")
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| default_name.to_string());
                let Some(bytes) = bytes(&data) else { return };
                crate::web::push_event(WebEvent::OpenDoc { name, bytes });
                ctx.request_repaint();
            });
            if self.window.add_event_listener_with_callback("message", on_message.as_ref().unchecked_ref()).is_ok() {
                on_message.forget();
            }
        }

        pub fn post_ready(&self, version: &str) {
            let ty = message_type(self.prefix, "ready");
            self.send(&[("type", ty.into()), ("version", version.into())]);
        }

        pub fn post_failed(&self, error: &str) {
            let ty = message_type(self.prefix, "failed");
            self.send(&[("type", ty.into()), ("error", error.into())]);
        }

        pub fn post_dirty(&self, dirty: bool) {
            if self.reported_dirty.get() == Some(dirty) {
                return;
            }
            let ty = message_type(self.prefix, "dirty");
            if self.send(&[("type", ty.into()), ("dirty", dirty.into())]) {
                self.reported_dirty.set(Some(dirty));
            }
        }

        pub fn post_save(&self, path: &str, bytes: &[u8], save_as: bool) -> Result<(), String> {
            let name = std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string());
            let buffer = js_sys::Uint8Array::from(bytes).buffer();
            let ty = message_type(self.prefix, "save");
            let msg = message(&[
                ("type", ty.into()),
                ("name", name.as_str().into()),
                ("saveAs", save_as.into()),
                ("bytes", buffer.clone().into()),
            ])
            .ok_or_else(|| "couldn't build host save message".to_string())?;
            self.parent
                .post_message_with_transfer(&msg, &self.origin, &js_sys::Array::of1(&buffer))
                .map_err(|e| e.as_string().unwrap_or_else(|| format!("{e:?}")))
        }

        fn send(&self, fields: &[(&str, JsValue)]) -> bool {
            message(fields).is_some_and(|m| self.parent.post_message(&m, &self.origin).is_ok())
        }
    }

    fn message(fields: &[(&str, JsValue)]) -> Option<js_sys::Object> {
        let m = js_sys::Object::new();
        for (key, value) in fields {
            js_sys::Reflect::set(&m, &JsValue::from_str(key), value).ok()?;
        }
        Some(m)
    }

    fn string(data: &JsValue, key: &str) -> Option<String> {
        js_sys::Reflect::get(data, &JsValue::from_str(key)).ok()?.as_string()
    }

    fn bytes(data: &JsValue) -> Option<Vec<u8>> {
        let val = js_sys::Reflect::get(data, &JsValue::from_str("bytes")).ok()?;
        if let Some(arr) = val.dyn_ref::<js_sys::Uint8Array>() {
            Some(arr.to_vec())
        } else if val.is_instance_of::<js_sys::ArrayBuffer>() {
            Some(js_sys::Uint8Array::new(&val).to_vec())
        } else {
            None
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use imp::Bridge;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_message_types_use_app_prefix() {
        assert_eq!(message_type("keydeck", "ready"), "keydeck:ready");
        assert_eq!(message_type("values", "open"), "values:open");
        assert_eq!(message_type("letters", "save"), "letters:save");
        assert_eq!(message_type("letters", "dirty"), "letters:dirty");
        assert_eq!(message_type("keydeck", "failed"), "keydeck:failed");
    }

    #[test]
    fn sanitize_author_trims_and_clamps() {
        assert_eq!(sanitize_author("   "), None);
        assert_eq!(sanitize_author("  Ada Lovelace "), Some("Ada Lovelace".into()));
        let long = "x".repeat(MAX_AUTHOR_CHARS + 50);
        assert_eq!(sanitize_author(&long).unwrap().len(), MAX_AUTHOR_CHARS);
    }
}
