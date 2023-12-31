use linkify::{LinkFinder, LinkKind};
use std::sync::Arc;
use twilight_http::Client;
use twilight_model::channel::message::MessageFlags;
use twilight_model::http::interaction::{InteractionResponse, InteractionResponseType};
use twilight_model::{
    application::interaction::InteractionData,
    channel::message::Embed,
    gateway::payload::incoming::InteractionCreate,
    id::{marker::ApplicationMarker, Id},
};
use twilight_util::builder::{
    embed::{EmbedBuilder, ImageSource},
    InteractionResponseDataBuilder,
};
use url::Url;

pub async fn handle_expand_threads_link(
    interaction: &InteractionCreate,
    application_id: Id<ApplicationMarker>,
    http_client: Arc<Client>,
    web_client: &reqwest::Client,
) -> Result<(), anyhow::Error> {
    let threads_links = parse_threads_links(interaction);
    let interaction_client = http_client.interaction(application_id);

    if threads_links.is_empty() {
        let interaction_response_data = InteractionResponseDataBuilder::new()
            .content("Sorry, there are no Threads links in this message.")
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
            .content("Loading Threads link info...")
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

    // TODO: build embeds for all links, not just the first one
    let first_link = threads_links.first().unwrap();
    let html = web_client
        .get(first_link.as_str())
        .send()
        .await?
        .text()
        .await?;
    let parsed_html = scraper::Html::parse_document(&html);

    let embed = build_threads_embed_from_html(&parsed_html)?;

    let embeds: Vec<Embed> = vec![embed];

    interaction_client
        .update_response(&interaction.token)
        .embeds(Some(&embeds))?
        .await?;

    Ok(())
}

fn build_threads_embed_from_html(parsed_html: &scraper::Html) -> Result<Embed, anyhow::Error> {
    let threads_title = meta_tag_content(parsed_html, "property", "og:title").unwrap_or_default();
    let threads_url = meta_tag_content(parsed_html, "property", "og:url").unwrap_or_default();
    let threads_description =
        meta_tag_content(parsed_html, "property", "og:description").unwrap_or_default();
    let threads_image = meta_tag_content(parsed_html, "property", "og:image").unwrap_or_default();
    let threads_image_type =
        meta_tag_content(parsed_html, "name", "twitter:card").unwrap_or_default();
    let image_is_profile_avatar = threads_image_type == "summary";

    let embed_builder = EmbedBuilder::new()
        .title(threads_title)
        .url(threads_url)
        .description(threads_description);

    let embed_builder = if image_is_profile_avatar {
        embed_builder.thumbnail(ImageSource::url(threads_image)?)
    } else {
        embed_builder.image(ImageSource::url(threads_image)?)
    };

    Ok(embed_builder.validate()?.build())
}

fn parse_threads_links(interaction: &InteractionCreate) -> Vec<Url> {
    let Some(InteractionData::ApplicationCommand(command_data)) = &interaction.data else {
        return Vec::new();
    };
    let Some(resolved_command_data) = &command_data.resolved else {
        return Vec::new();
    };
    let finder = LinkFinder::new();

    let links_iterator = resolved_command_data
        .messages
        .values()
        .map(|m| &m.content)
        .flat_map(|content| {
            finder
                .links(content)
                .filter(|l| l.kind() == &LinkKind::Url)
                .filter_map(|l| Url::parse(l.as_str()).ok())
                .filter(|l| {
                    if let Some(host_str) = l.host_str() {
                        host_str == "threads.net" || host_str == "www.threads.net"
                    } else {
                        false
                    }
                })
        });
    links_iterator.collect()
}

fn meta_tag_content<'html>(
    parsed_html: &'html scraper::Html,
    attr_name: &str,
    attr_value: &str,
) -> Option<&'html str> {
    let tag_selector =
        scraper::Selector::parse(&format!(r#"meta[{}="{}"]"#, attr_name, attr_value)).ok()?;
    parsed_html
        .select(&tag_selector)
        .next()
        .and_then(|element_ref| element_ref.attr("content"))
}
