use yew::prelude::*;

use super::components::{Icon, RoundIcon};
use crate::data::{DetailTab, EventEntry};

#[derive(Properties, PartialEq)]
pub struct SidePanelProps {
    pub active: DetailTab,
    pub on_select: Callback<DetailTab>,
    pub events: Vec<EventEntry>,
}

#[function_component(SidePanel)]
pub fn side_panel(props: &SidePanelProps) -> Html {
    html! {
        <aside class="adk-side-panel">
            <nav class="adk-tabs" role="tablist">
                {for DetailTab::ALL.into_iter().map(|tab| tab_button(tab, props.active, &props.on_select))}
            </nav>
            <div class="adk-event-nav">
                <button type="button"><Icon name="chevron_left" /></button>
                <button type="button"><Icon name="chevron_right" /></button>
                <span>{format!("Event {} of {}", props.events.len(), props.events.len())}</span>
                <button type="button"><Icon name="remove" /></button>
            </div>
            <div class="adk-side-body">
                <div class="adk-rail">
                    <RoundIcon name="info" active={matches!(props.active, DetailTab::Info)} />
                    <RoundIcon name="account_tree" active=true />
                    <RoundIcon name="input" />
                    <RoundIcon name="output" />
                    <RoundIcon name="analytics" />
                    <RoundIcon name="published_with_changes" />
                    <RoundIcon name="data_object" />
                </div>
                <section class="adk-side-content">
                    {side_content(props.active, &props.events)}
                </section>
            </div>
        </aside>
    }
}

fn tab_button(tab: DetailTab, active: DetailTab, on_select: &Callback<DetailTab>) -> Html {
    let selected = tab == active;
    let class = if selected {
        "adk-tab active"
    } else {
        "adk-tab"
    };
    let on_select = on_select.clone();
    html! {
        <button class={class} role="tab" aria-selected={selected.to_string()} onclick={Callback::from(move |_| on_select.emit(tab))}>
            {tab.label()}
        </button>
    }
}

fn side_content(tab: DetailTab, events: &[EventEntry]) -> Html {
    match tab {
        DetailTab::Info => html! { <><Invocation events={events.len()} /><Graph /></> },
        DetailTab::State => html! { <StateView events={events.to_vec()} /> },
        DetailTab::Artifacts => {
            html! { <EmptyPane title="No artifacts" body="Artifacts from tool responses will appear here." /> }
        }
        DetailTab::Evals => {
            html! { <EmptyPane title="No eval sets" body="Eval cases and results will appear here." /> }
        }
    }
}

#[derive(Properties, PartialEq)]
struct InvocationProps {
    events: usize,
}

#[function_component(Invocation)]
fn invocation(props: &InvocationProps) -> Html {
    html! {
        <div class="adk-invocation-row">
            <strong>{"Invocation:"}</strong>
            <span>{format!("#1 ({} events)", props.events)}</span>
            <Icon name="arrow_drop_down" />
        </div>
    }
}

#[function_component(Graph)]
fn graph() -> Html {
    html! {
        <div class="adk-graph-card">
            <button class="adk-fullscreen" type="button" aria-label="Fullscreen"><Icon name="fullscreen" /></button>
            <svg viewBox="0 0 420 250" role="img" aria-label="hello_world agent graph">
                <rect width="420" height="250" fill="#0F172A" />
                <line x1="210" y1="88" x2="120" y2="165" stroke="#94A3B8" stroke-width="2" stroke-dasharray="7 6" />
                <line x1="210" y1="88" x2="300" y2="165" stroke="#94A3B8" stroke-width="2" stroke-dasharray="7 6" />
                <rect x="80" y="35" width="260" height="54" rx="18" fill="#14532d" stroke="#bbf7d0" stroke-width="6" />
                <text x="210" y="69" text-anchor="middle" fill="#dcfce7" font-size="18">{"✦ hello_world_agent"}</text>
                <rect x="55" y="158" width="130" height="50" rx="18" fill="#1E293B" stroke="#94A3B8" stroke-width="2" stroke-dasharray="8 6" />
                <text x="120" y="189" text-anchor="middle" fill="#CBD5E1" font-size="16">{"roll_die"}</text>
                <rect x="235" y="158" width="140" height="50" rx="18" fill="#1E293B" stroke="#94A3B8" stroke-width="2" stroke-dasharray="8 6" />
                <text x="305" y="189" text-anchor="middle" fill="#CBD5E1" font-size="16">{"check_prime"}</text>
            </svg>
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct StateViewProps {
    events: Vec<EventEntry>,
}

#[function_component(StateView)]
fn state_view(props: &StateViewProps) -> Html {
    let chips = props
        .events
        .iter()
        .flat_map(|event| event.chips.iter())
        .cloned()
        .collect::<Vec<_>>();
    html! { <div class="adk-state-list">{for chips.into_iter().map(|chip| html!{<span>{chip}</span>})}</div> }
}

#[derive(Properties, PartialEq)]
struct EmptyPaneProps {
    title: &'static str,
    body: &'static str,
}

#[function_component(EmptyPane)]
fn empty_pane(props: &EmptyPaneProps) -> Html {
    html! { <div class="adk-empty-pane"><h3>{props.title}</h3><p>{props.body}</p></div> }
}
