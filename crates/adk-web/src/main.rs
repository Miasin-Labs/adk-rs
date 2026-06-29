#[cfg(target_arch = "wasm32")]
mod api;
#[cfg(target_arch = "wasm32")]
mod data;
#[cfg(target_arch = "wasm32")]
mod views;

#[cfg(target_arch = "wasm32")]
fn main() {
    yew::Renderer::<views::App>::new().render();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    println!("adk-web is a Yew app. Run with: trunk serve --open");
}
