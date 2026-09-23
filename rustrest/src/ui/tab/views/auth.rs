//! The Authorization tab: a type dropdown, and the field set for whichever type is selected.

use super::super::Tab;
use super::super::messages::{AuthMessage, TabMessage};
use crate::message::MultilineFieldKind;
use crate::ui::context_menu::TabFieldTarget;
use crate::ui::multiline_input::multiline_input;
use crate::ui::spinner::spinner_with_label;
use iced::widget::{Space, button, column, pick_list, row, text, text_input};
use iced::{Alignment, Element, Length};
use rustrest_core::{
    AuthLocation, AuthType, ClientAuthStyle, JwtAlgorithm, OAuth1SignatureMethod, OAuth2GrantType,
};

const LABEL_WIDTH: f32 = 150.0;

fn field_row<'a, Message: 'a>(label: &'a str, input: Element<'a, Message>) -> Element<'a, Message> {
    row![
        text(label).size(12).width(Length::Fixed(LABEL_WIDTH)),
        input
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn hint<'a, Message: 'a>(text_str: &'a str) -> Element<'a, Message> {
    text(text_str)
        .size(11)
        .color(iced::Color::from_rgb(0.5, 0.5, 0.5))
        .into()
}

#[allow(clippy::too_many_arguments)]
pub fn render_auth_pane<'a, Message>(
    tab: &'a Tab,
    wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
    multiline_height: impl Fn(MultilineFieldKind) -> f32 + Copy + 'a,
    on_multiline_resize_start: impl Fn(MultilineFieldKind) -> Message + Copy + 'a,
    spinner_tick: u64,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let tab_id = tab.id;
    let form = &tab.request_auth;
    let wrap_auth = move |m: AuthMessage| wrap_msg(TabMessage::Auth(m));

    let type_picker = pick_list(&AuthType::ALL[..], Some(form.auth_type), move |t| {
        wrap_auth(AuthMessage::TypeChanged(t))
    })
    .padding(8);

    let content = column![type_picker].spacing(14).width(Length::Fill);

    let fields: Element<'a, Message> = match form.auth_type {
        AuthType::NoAuth => text("This request does not use any authorization.")
            .color(iced::Color::from_rgb(0.5, 0.5, 0.5))
            .into(),

        AuthType::Custom => multiline_input(
            "Authorization: ...",
            &form.custom_raw,
            10,
            multiline_height(MultilineFieldKind::Auth(tab_id)),
            move |action| wrap_auth(AuthMessage::CustomRawAction(action)),
            wrap_msg(TabMessage::ShowFieldContextMenu(
                TabFieldTarget::AuthCustom,
                form.custom_raw.text(),
            )),
            on_multiline_resize_start(MultilineFieldKind::Auth(tab_id)),
        ),

        AuthType::Bearer => column![field_row(
            "Token",
            text_input("your_token_here", &form.bearer_token)
                .on_input(move |v| wrap_auth(AuthMessage::BearerTokenChanged(v)))
                .padding(8)
                .into(),
        )]
        .spacing(10)
        .into(),

        AuthType::ApiKey => column![
            field_row(
                "Key",
                text_input("X-API-Key", &form.api_key_key)
                    .on_input(move |v| wrap_auth(AuthMessage::ApiKeyKeyChanged(v)))
                    .padding(8)
                    .into(),
            ),
            field_row(
                "Value",
                text_input("your_api_key", &form.api_key_value)
                    .on_input(move |v| wrap_auth(AuthMessage::ApiKeyValueChanged(v)))
                    .padding(8)
                    .into(),
            ),
            field_row(
                "Add to",
                pick_list(&AuthLocation::ALL[..], Some(form.api_key_add_to), move |v| {
                    wrap_auth(AuthMessage::ApiKeyAddToChanged(v))
                })
                .padding(8)
                .into(),
            ),
        ]
        .spacing(10)
        .into(),

        AuthType::Basic => column![
            field_row(
                "Username",
                text_input("username", &form.basic_username)
                    .on_input(move |v| wrap_auth(AuthMessage::BasicUsernameChanged(v)))
                    .padding(8)
                    .into(),
            ),
            field_row(
                "Password",
                text_input("password", &form.basic_password)
                    .on_input(move |v| wrap_auth(AuthMessage::BasicPasswordChanged(v)))
                    .secure(true)
                    .padding(8)
                    .into(),
            ),
        ]
        .spacing(10)
        .into(),

        AuthType::JwtBearer => {
            let payload_editor = multiline_input(
                r#"{"sub": "1234567890"}"#,
                &form.jwt_payload,
                10,
                multiline_height(MultilineFieldKind::AuthJwtPayload(tab_id)),
                move |action| wrap_auth(AuthMessage::JwtPayloadAction(action)),
                wrap_msg(TabMessage::ShowFieldContextMenu(
                    TabFieldTarget::AuthJwtPayload,
                    form.jwt_payload.text(),
                )),
                on_multiline_resize_start(MultilineFieldKind::AuthJwtPayload(tab_id)),
            );

            column![
                field_row(
                    "Algorithm",
                    pick_list(&JwtAlgorithm::ALL[..], Some(form.jwt_algorithm), move |v| {
                        wrap_auth(AuthMessage::JwtAlgorithmChanged(v))
                    })
                    .padding(8)
                    .into(),
                ),
                field_row(
                    "Secret",
                    text_input("your_secret", &form.jwt_secret)
                        .on_input(move |v| wrap_auth(AuthMessage::JwtSecretChanged(v)))
                        .secure(true)
                        .padding(8)
                        .into(),
                ),
                field_row(
                    "Header Prefix",
                    text_input("Bearer", &form.jwt_header_prefix)
                        .on_input(move |v| wrap_auth(AuthMessage::JwtHeaderPrefixChanged(v)))
                        .padding(8)
                        .into(),
                ),
                field_row(
                    "Add to",
                    pick_list(&AuthLocation::ALL[..], Some(form.jwt_add_to), move |v| {
                        wrap_auth(AuthMessage::JwtAddToChanged(v))
                    })
                    .padding(8)
                    .into(),
                ),
                text("Payload").size(12),
                payload_editor,
                hint("Signed locally (HS256/384/512) into a JWT and sent as the token - the payload above becomes its claims."),
            ]
            .spacing(10)
            .into()
        }

        AuthType::OAuth1 => column![
            field_row(
                "Signature Method",
                pick_list(
                    &OAuth1SignatureMethod::ALL[..],
                    Some(form.oauth1_signature_method),
                    move |v| wrap_auth(AuthMessage::OAuth1SignatureMethodChanged(v)),
                )
                .padding(8)
                .into(),
            ),
            field_row(
                "Consumer Key",
                text_input("consumer key", &form.oauth1_consumer_key)
                    .on_input(move |v| wrap_auth(AuthMessage::OAuth1ConsumerKeyChanged(v)))
                    .padding(8)
                    .into(),
            ),
            field_row(
                "Consumer Secret",
                text_input("consumer secret", &form.oauth1_consumer_secret)
                    .on_input(move |v| wrap_auth(AuthMessage::OAuth1ConsumerSecretChanged(v)))
                    .secure(true)
                    .padding(8)
                    .into(),
            ),
            field_row(
                "Access Token",
                text_input("token (optional)", &form.oauth1_token)
                    .on_input(move |v| wrap_auth(AuthMessage::OAuth1TokenChanged(v)))
                    .padding(8)
                    .into(),
            ),
            field_row(
                "Token Secret",
                text_input("token secret (optional)", &form.oauth1_token_secret)
                    .on_input(move |v| wrap_auth(AuthMessage::OAuth1TokenSecretChanged(v)))
                    .secure(true)
                    .padding(8)
                    .into(),
            ),
            field_row(
                "Realm",
                text_input("realm (optional)", &form.oauth1_realm)
                    .on_input(move |v| wrap_auth(AuthMessage::OAuth1RealmChanged(v)))
                    .padding(8)
                    .into(),
            ),
            field_row(
                "Add params to",
                pick_list(&AuthLocation::ALL[..], Some(form.oauth1_add_to), move |v| {
                    wrap_auth(AuthMessage::OAuth1AddToChanged(v))
                })
                .padding(8)
                .into(),
            ),
            hint("Signs the request per RFC 5849. RSA-SHA1 isn't supported - use HMAC-SHA1 or PLAINTEXT."),
        ]
        .spacing(10)
        .into(),

        AuthType::OAuth2 => {
            let mut col = column![field_row(
                "Grant Type",
                pick_list(&OAuth2GrantType::ALL[..], Some(form.oauth2_grant_type), move |v| {
                    wrap_auth(AuthMessage::OAuth2GrantTypeChanged(v))
                })
                .padding(8)
                .into(),
            )]
            .spacing(10);

            if form.oauth2_grant_type == OAuth2GrantType::ClientCredentials {
                col = col
                    .push(field_row(
                        "Token URL",
                        text_input("https://auth.example.com/oauth/token", &form.oauth2_token_url)
                            .on_input(move |v| wrap_auth(AuthMessage::OAuth2TokenUrlChanged(v)))
                            .padding(8)
                            .into(),
                    ))
                    .push(field_row(
                        "Client ID",
                        text_input("client id", &form.oauth2_client_id)
                            .on_input(move |v| wrap_auth(AuthMessage::OAuth2ClientIdChanged(v)))
                            .padding(8)
                            .into(),
                    ))
                    .push(field_row(
                        "Client Secret",
                        text_input("client secret", &form.oauth2_client_secret)
                            .on_input(move |v| wrap_auth(AuthMessage::OAuth2ClientSecretChanged(v)))
                            .secure(true)
                            .padding(8)
                            .into(),
                    ))
                    .push(field_row(
                        "Scope",
                        text_input("scope (optional)", &form.oauth2_scope)
                            .on_input(move |v| wrap_auth(AuthMessage::OAuth2ScopeChanged(v)))
                            .padding(8)
                            .into(),
                    ))
                    .push(field_row(
                        "Client Authentication",
                        pick_list(
                            &ClientAuthStyle::ALL[..],
                            Some(form.oauth2_client_auth),
                            move |v| wrap_auth(AuthMessage::OAuth2ClientAuthChanged(v)),
                        )
                        .padding(8)
                        .into(),
                    ));

                let fetch_control: Element<'a, Message> = if form.oauth2_fetching_token {
                    spinner_with_label(spinner_tick, "Fetching token...")
                } else {
                    let can_fetch = !form.oauth2_token_url.trim().is_empty()
                        && !form.oauth2_client_id.trim().is_empty();
                    button(text("Get New Access Token").size(13))
                        .style(button::secondary)
                        .padding([6, 12])
                        .on_press_maybe(
                            can_fetch.then(|| wrap_auth(AuthMessage::OAuth2FetchToken)),
                        )
                        .into()
                };
                col = col.push(row![fetch_control, Space::new().width(Length::Fill)]);
            }

            col = col
                .push(field_row(
                    "Access Token",
                    text_input("access token", &form.oauth2_access_token)
                        .on_input(move |v| wrap_auth(AuthMessage::OAuth2AccessTokenChanged(v)))
                        .padding(8)
                        .into(),
                ))
                .push(field_row(
                    "Header Prefix",
                    text_input("Bearer", &form.oauth2_header_prefix)
                        .on_input(move |v| wrap_auth(AuthMessage::OAuth2HeaderPrefixChanged(v)))
                        .padding(8)
                        .into(),
                ))
                .push(field_row(
                    "Add to",
                    pick_list(&AuthLocation::ALL[..], Some(form.oauth2_add_to), move |v| {
                        wrap_auth(AuthMessage::OAuth2AddToChanged(v))
                    })
                    .padding(8)
                    .into(),
                ));

            if form.oauth2_access_token.trim().is_empty() {
                col = col.push(hint(
                    "No access token yet - paste one, or use Client Credentials and fetch one above.",
                ));
            }

            col.into()
        }
    };

    content.push(fields).into()
}
