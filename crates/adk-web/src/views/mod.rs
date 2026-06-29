mod chat;
mod components;
mod side_panel;
mod top_bar;

use chat::ChatPanel;
use side_panel::SidePanel;
use top_bar::TopBar;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::api;
use crate::data::{DetailTab, EventEntry};

#[function_component(App)]
pub fn app() -> Html {
    let active_tab = use_state(|| DetailTab::Info);
    let events = use_state(Vec::<EventEntry>::new);
    let input = use_state(String::new);
    let session_id = use_state(|| None::<String>);
    let pending = use_state(|| false);

    let on_send = send_callback(&input, &events, &session_id, &pending);

    html! {
        <main class="adk-dev-shell">
            <TopBar session_title={session_title(&events)} />
            <section class="adk-workspace">
                <SidePanel active={*active_tab} on_select={tab_callback(&active_tab)} events={(*events).clone()} />
                <ChatPanel
                    events={(*events).clone()}
                    input={(*input).clone()}
                    pending={*pending}
                    on_input={input_callback(&input)}
                    on_send={on_send}
                />
            </section>
        </main>
    }
}

fn send_callback(
    input: &UseStateHandle<String>,
    events: &UseStateHandle<Vec<EventEntry>>,
    session_id: &UseStateHandle<Option<String>>,
    pending: &UseStateHandle<bool>,
) -> Callback<()> {
    let input = input.clone();
    let events = events.clone();
    let session_id = session_id.clone();
    let pending = pending.clone();
    Callback::from(move |_| {
        let prompt = input.trim().to_owned();
        if prompt.is_empty() || *pending {
            return;
        }
        let mut next = (*events).clone();
        next.push(EventEntry::user(&prompt));
        events.set(next.clone());
        input.set(String::new());
        pending.set(true);
        spawn_local(run_prompt(
            prompt,
            next,
            events.clone(),
            session_id.clone(),
            pending.clone(),
        ));
    })
}

async fn run_prompt(
    prompt: String,
    base_events: Vec<EventEntry>,
    events: UseStateHandle<Vec<EventEntry>>,
    session_id: UseStateHandle<Option<String>>,
    pending: UseStateHandle<bool>,
) {
    let id = match (*session_id).clone() {
        Some(id) => id,
        None => match api::create_session().await {
            Ok(id) => {
                session_id.set(Some(id.clone()));
                id
            }
            Err(error) => {
                set_events(&events, base_events, vec![EventEntry::error(error)]);
                pending.set(false);
                return;
            }
        },
    };
    match api::run_sse(&id, &prompt).await {
        Ok(rows) => set_events(&events, base_events, rows),
        Err(error) => set_events(&events, base_events, vec![EventEntry::error(error)]),
    }
    pending.set(false);
}

fn set_events(
    events: &UseStateHandle<Vec<EventEntry>>,
    mut next: Vec<EventEntry>,
    rows: Vec<EventEntry>,
) {
    next.extend(rows);
    events.set(next);
}

fn tab_callback(active_tab: &UseStateHandle<DetailTab>) -> Callback<DetailTab> {
    let active_tab = active_tab.clone();
    Callback::from(move |tab| active_tab.set(tab))
}

fn input_callback(input: &UseStateHandle<String>) -> Callback<String> {
    let input = input.clone();
    Callback::from(move |value| input.set(value))
}

fn session_title(events: &[EventEntry]) -> String {
    events
        .iter()
        .find(|event| matches!(event.kind, crate::data::EventKind::User))
        .map(|event| event.text.clone())
        .unwrap_or_else(|| "NEW SESSION".to_owned())
}
