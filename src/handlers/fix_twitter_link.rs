use std::sync::Arc;

use linkify::{Link, LinkFinder, LinkKind};
use twilight_http::Client;
use twilight_model::{
    application::{
        command::CommandType,
        interaction::{application_command::CommandOptionValue, InteractionData},
    },
    channel::message::MessageFlags,
    gateway::payload::incoming::InteractionCreate,
    http::interaction::{InteractionResponse, InteractionResponseType},
    id::{marker::ApplicationMarker, Id},
};
use twilight_util::builder::InteractionResponseDataBuilder;
use url::Url;

pub async fn handle_fix_twitter_link(
    interaction: &InteractionCreate,
    application_id: Id<ApplicationMarker>,
    http_client: Arc<Client>,
) -> Result<(), anyhow::Error> {
    let (original_message, twitter_links) = parse_twitter_links(interaction);
    let interaction_client = http_client.interaction(application_id);

    if twitter_links.is_empty() {
        let interaction_response_data = InteractionResponseDataBuilder::new()
            .content("Sorry, there are no Twitter links in this message.")
            .flags(MessageFlags::EPHEMERAL)
            .build();
        interaction_client
            .create_response(
                interaction.id,
                &interaction.token,
                &InteractionResponse {
                    kind: InteractionResponseType::ChannelMessageWithSource,
                    data: Some(interaction_response_data),
                },
            )
            .await?;
        return Ok(());
    } else {
        let interaction_response_data = InteractionResponseDataBuilder::new()
            .content("Fixing Twitter links...")
            .build();
        interaction_client
            .create_response(
                interaction.id,
                &interaction.token,
                &InteractionResponse {
                    kind: InteractionResponseType::DeferredChannelMessageWithSource,
                    data: Some(interaction_response_data),
                },
            )
            .await?;
    }

    let new_message_content = fix_twitter_links_in_place(original_message.clone(), twitter_links);

    interaction_client
        .update_response(&interaction.token)
        .content(Some(&new_message_content))?
        .await?;

    Ok(())
}

/// Returns (message_content, link_information). Does not handle more than one message.
fn parse_twitter_links(interaction: &InteractionCreate) -> (String, Vec<(Url, Link)>) {
    let Some(message_content) = message_content_from_interaction(interaction) else {
        return (String::new(), Vec::new());
    };
    let parsed_link_information = parse_twitter_links_inner(message_content);

    (message_content.to_string(), parsed_link_information)
}

// We need to handle interactions coming from both slash commands and message
// commands. Those require some different plumbing to get to the input message,
// which we take care of here.
fn message_content_from_interaction(interaction: &InteractionCreate) -> Option<&str> {
    let Some(InteractionData::ApplicationCommand(command_data)) = &interaction.data else {
        return None;
    };

    match command_data.kind {
        CommandType::ChatInput => command_data
            .options
            .iter()
            .next()
            .map(|opt| &opt.value)
            .and_then(|value| match value {
                CommandOptionValue::String(value) => Some(value.as_str()),
                _ => None,
            }),
        CommandType::Message => command_data
            .resolved
            .as_ref()
            .and_then(|resolved_command_data| {
                resolved_command_data
                    .messages
                    .iter()
                    .next()
                    .map(|(_, m)| m)
                    .map(|m| m.content.as_str())
            }),
        _ => None,
    }
}

fn parse_twitter_links_inner(message_content: &str) -> Vec<(Url, Link)> {
    let finder = LinkFinder::new();
    let links_iterator = finder
        .links(message_content)
        .filter(|l| l.kind() == &LinkKind::Url)
        .filter_map(|l| Url::parse(l.as_str()).ok().zip(Some(l)))
        .filter(|(u, _)| {
            if let Some(host_str) = u.host_str() {
                host_str == "twitter.com"
                    || host_str == "mobile.twitter.com"
                    || host_str == "x.com"
                    || host_str == "mobile.x.com"
            } else {
                false
            }
        });
    links_iterator.collect()
}

fn fix_twitter_links_in_place(
    original_message: String,
    twitter_links: Vec<(Url, Link<'_>)>,
) -> String {
    let mut new_message_content = original_message;

    for (mut parsed_url, link_info) in twitter_links.into_iter().rev() {
        let Some(host) = parsed_url.host_str() else {
            continue;
        };

        let new_host = match host {
            "twitter.com" => "fxtwitter.com",
            "mobile.twitter.com" => "fxtwitter.com",
            "x.com" => "fixupx.com",
            "mobile.x.com" => "fixupx.com",
            _ => "fxtwitter.com",
        };
        if let Err(_) = parsed_url.set_host(Some(new_host)) {
            continue;
        }

        new_message_content.replace_range(link_info.start()..link_info.end(), parsed_url.as_str());
    }
    new_message_content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_twitter_links_in_place() {
        let original_message = r#"
            This is a message that contains a twitter link (https://twitter.com/test/test), a
            x.com link (https://x.com/test/test), a mobile.twitter.com link
            (https://mobile.twitter.com/test/test), a mobile.x.com link
            (https://mobile.x.com/test/test), an unrelated link
            (https://otherwebsite/test/test), and a weird twitter link
            (https://weird.link.twitter.com/test/test).
        "#;
        let twitter_links = parse_twitter_links_inner(&original_message);

        let new_message = fix_twitter_links_in_place(original_message.to_string(), twitter_links);
        assert_eq!(
            new_message,
            r#"
            This is a message that contains a twitter link (https://fxtwitter.com/test/test), a
            x.com link (https://fixupx.com/test/test), a mobile.twitter.com link
            (https://fxtwitter.com/test/test), a mobile.x.com link
            (https://fixupx.com/test/test), an unrelated link
            (https://otherwebsite/test/test), and a weird twitter link
            (https://weird.link.twitter.com/test/test).
        "#
        );
    }
}
