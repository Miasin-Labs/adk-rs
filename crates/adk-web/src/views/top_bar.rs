use yew::prelude::*;

use super::components::Icon;
use crate::data::APP_NAME;

#[derive(Properties, PartialEq)]
pub struct TopBarProps {
    pub session_title: String,
}

#[function_component(TopBar)]
pub fn top_bar(props: &TopBarProps) -> Html {
    html! {
        <header class="adk-topbar">
            <div class="adk-topbar-left">
                <button class="adk-icon-button" type="button" aria-label="Toggle side panel"><Icon name="menu" /></button>
                <div class="adk-brand-mark" aria-hidden="true">{"<>"}</div>
                <strong class="adk-product">{"Agent Development Kit "}<span>{"2.2.0"}</span></strong>
                <button class="adk-selector" type="button"><Icon name="robot_2" />{APP_NAME}<Icon name="arrow_drop_down" /></button>
                <button class="adk-icon-button bordered" type="button" aria-label="Agent graph"><Icon name="account_tree" /></button>
                <button class="adk-icon-button bordered" type="button" aria-label="Builder"><Icon name="edit" /></button>
                <button class="adk-selector session" type="button"><Icon name="chat" />{props.session_title.clone()}<Icon name="arrow_drop_down" /></button>
                <button class="adk-new-session" type="button"><Icon name="add_comment" />{"New Session"}</button>
            </div>
            <div class="adk-topbar-right">
                <button class="adk-icon-button" type="button" aria-label="Theme"><Icon name="light_mode" /></button>
                <button class="adk-icon-button" type="button" aria-label="User menu"><Icon name="account_circle" /></button>
            </div>
        </header>
    }
}
