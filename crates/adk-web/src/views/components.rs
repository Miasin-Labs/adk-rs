use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct IconProps {
    pub name: &'static str,
}

#[function_component(Icon)]
pub fn icon(props: &IconProps) -> Html {
    html! { <span class="material-symbols-outlined adk-symbol">{props.name}</span> }
}

#[derive(Properties, PartialEq)]
pub struct ChipProps {
    pub text: String,
}

#[function_component(Chip)]
pub fn chip(props: &ChipProps) -> Html {
    html! { <span class="adk-chip">{props.text.clone()}</span> }
}

#[derive(Properties, PartialEq)]
pub struct RoundIconProps {
    pub name: &'static str,
    #[prop_or(false)]
    pub active: bool,
}

#[function_component(RoundIcon)]
pub fn round_icon(props: &RoundIconProps) -> Html {
    let class = if props.active {
        "adk-rail-button adk-rail-button-active"
    } else {
        "adk-rail-button"
    };
    html! { <button class={class} type="button" aria-label={props.name}><Icon name={props.name} /></button> }
}
