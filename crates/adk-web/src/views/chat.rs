use web_sys::HtmlTextAreaElement;
use yew::TargetCast;
use yew::prelude::*;

use super::components::{Chip, Icon};
use crate::data::{EventEntry, EventKind};

#[derive(Properties, PartialEq)]
pub struct ChatPanelProps {
    pub events: Vec<EventEntry>,
    pub input: String,
    pub pending: bool,
    pub on_input: Callback<String>,
    pub on_send: Callback<()>,
}

#[function_component(ChatPanel)]
pub fn chat_panel(props: &ChatPanelProps) -> Html {
    html! {
        <section class="adk-chat-card">
            <ChatToolbar />
            <div class="adk-events">
                {for props.events.iter().enumerate().map(|(index, event)| event_row(index + 1, event))}
                {if props.pending { html! { <div class="adk-pending">{"Waiting for OpenAI and tools..."}</div> } } else { Html::default() }}
            </div>
            <Composer input={props.input.clone()} on_input={props.on_input.clone()} on_send={props.on_send.clone()} pending={props.pending} />
        </section>
    }
}

#[function_component(ChatToolbar)]
fn chat_toolbar() -> Html {
    html! {
        <div class="adk-chat-toolbar">
            <div class="adk-segmented"><button class="active" type="button">{"Events"}</button><button type="button">{"Traces"}</button></div>
            <button class="adk-filter" type="button"><Icon name="add" />{"Filter"}</button>
            <span class="grow" />
            <button class="adk-icon-button" type="button" aria-label="Refresh"><Icon name="refresh" /></button>
            <button class="adk-icon-button" type="button" aria-label="More options"><Icon name="more_vert" /></button>
        </div>
    }
}

fn event_row(index: usize, event: &EventEntry) -> Html {
    let class = match event.kind {
        EventKind::User => "adk-event-row user",
        EventKind::Agent => "adk-event-row agent selected",
        EventKind::ToolCall => "adk-event-row tool",
        EventKind::ToolResponse => "adk-event-row tool-response",
        EventKind::Error => "adk-event-row error",
    };
    html! {
        <article class={class}>
            <span class="adk-event-index">{format!("#{index}")}</span>
            <Avatar kind={event.kind.clone()} />
            <div class="adk-event-content">
                {event_body(event)}
                <div class="adk-chip-row">{for event.chips.iter().cloned().map(|text| html!{<Chip text={text} />})}</div>
            </div>
        </article>
    }
}

fn event_body(event: &EventEntry) -> Html {
    match event.kind {
        EventKind::ToolCall => {
            html! { <button class="adk-tool-pill" type="button"><Icon name="bolt" />{event.text.clone()}</button> }
        }
        EventKind::ToolResponse => {
            html! { <button class="adk-tool-pill ok" type="button"><Icon name="check" />{event.text.clone()}</button> }
        }
        EventKind::User => html! { <p class="adk-user-bubble">{event.text.clone()}</p> },
        EventKind::Agent | EventKind::Error => html! { <p>{event.text.clone()}</p> },
    }
}

#[derive(Properties, PartialEq)]
struct AvatarProps {
    kind: EventKind,
}

#[function_component(Avatar)]
fn avatar(props: &AvatarProps) -> Html {
    match props.kind {
        EventKind::User => html! { <div class="adk-avatar user"><Icon name="person" /></div> },
        EventKind::Error => html! { <div class="adk-avatar error"><Icon name="error" /></div> },
        _ => html! { <div class="adk-avatar agent"><Icon name="robot_2" /></div> },
    }
}

#[derive(Properties, PartialEq)]
struct ComposerProps {
    input: String,
    pending: bool,
    on_input: Callback<String>,
    on_send: Callback<()>,
}

#[function_component(Composer)]
fn composer(props: &ComposerProps) -> Html {
    let on_input = props.on_input.clone();
    let oninput = Callback::from(move |event: InputEvent| {
        let input = event.target_unchecked_into::<HtmlTextAreaElement>();
        on_input.emit(input.value());
    });
    let on_send = props.on_send.clone();
    let onclick = Callback::from(move |_| on_send.emit(()));
    let on_send = props.on_send.clone();
    let onkeydown = Callback::from(move |event: KeyboardEvent| {
        if event.key() == "Enter" && !event.shift_key() {
            event.prevent_default();
            on_send.emit(());
        }
    });
    html! {
        <div class="adk-composer">
            <button type="button"><Icon name="add" /></button>
            <textarea value={props.input.clone()} {oninput} {onkeydown} placeholder="Type a message..." rows="1" />
            <button class="call" type="button"><Icon name="call" /></button>
            <button class="send" type="button" {onclick} disabled={props.pending}><Icon name="send" /></button>
        </div>
    }
}
