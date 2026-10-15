// Web bridge: the page calls `window.heritage.exec(method, params)` (set up in web/bootstrap.js),
// which queues an automation request and returns a promise of its result. Playwright drives the
// web build this way, and it works from the browser console too.
//
// The promise resolves with the command's result, or rejects with { code, message }.

use js_sys::{Function, JSON, Object, Promise, Reflect};
use serde_json::{Map, Value};
use wasm_bindgen::prelude::*;

use super::{Request, Responder, Response};
use crate::engine::Engine;

#[wasm_bindgen(js_name = heritageExec)]
pub fn heritage_exec(method: String, params: JsValue) -> Promise {
    let mut request = Some((method, params));

    Promise::new(&mut |resolve, reject| {
        let Some((method, params)) = request.take() else {
            return;
        };

        let params = match params_to_json(&params) {
            Ok(params) => params,
            Err(message) => return reject_with(&reject, "InvalidParams", &message),
        };

        if !Engine::is_initialized() {
            return reject_with(&reject, "NotReady", "The game is still loading.");
        }

        let responder = Responder::new(move |response: Response| match response.result {
            Ok(result) => {
                let result = JSON::parse(&result.to_string()).unwrap_or(JsValue::NULL);
                let _ = resolve.call1(&JsValue::NULL, &result);
            }
            Err(err) => reject_with(&reject, &err.code, &err.message),
        });

        Engine::get().automation_request_sender().send(Request { method, params, responder });
    })
}

fn params_to_json(params: &JsValue) -> Result<Value, String> {
    if params.is_undefined() || params.is_null() {
        return Ok(Value::Object(Map::new()));
    }

    let text = JSON::stringify(params).ok().and_then(|text| text.as_string()).ok_or("params can't be turned into JSON.")?;
    match serde_json::from_str::<Value>(&text) {
        Ok(params @ Value::Object(_)) => Ok(params),
        Ok(_) => Err("params must be an object.".into()),
        Err(err) => Err(format!("params aren't valid JSON: {err}")),
    }
}

fn reject_with(reject: &Function, code: &str, message: &str) {
    let error = Object::new();
    let _ = Reflect::set(&error, &JsValue::from_str("code"), &JsValue::from_str(code));
    let _ = Reflect::set(&error, &JsValue::from_str("message"), &JsValue::from_str(message));
    let _ = reject.call1(&JsValue::NULL, &error);
}
