#[cfg(feature = "csr")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn start() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}

pub mod app;
pub mod pages;
pub mod server_fns;
pub mod ws;
