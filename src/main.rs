use futures_util::StreamExt;
use handlers::{
    expand_threads_link::handle_expand_threads_link, fix_twitter_link::handle_fix_twitter_link,
};
use std::sync::Arc;
use twilight_gateway::{
    stream::{self, ShardEventStream},
    Config,
};
use twilight_http::Client;
use twilight_model::{
    application::command::{CommandOption, CommandOptionType},
    gateway::event::Event,
};
use twilight_model::{
    application::{command::CommandType, interaction::InteractionData},
    gateway::Intents,
};
use twilight_util::builder::command::CommandBuilder;

mod config;
mod handlers;

enum MessageCommands {
    ExpandThreadsLink,
    FixTwitterLink,
}

impl MessageCommands {
    fn as_str(&self) -> &str {
        match self {
            Self::ExpandThreadsLink => "Expand Threads link",
            Self::FixTwitterLink => "Fix Twitter link",
        }
    }
}

enum SlashCommands {
    FixTwitterLink,
}

impl SlashCommands {
    fn as_str(&self) -> &str {
        match self {
            Self::FixTwitterLink => "fx",
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::Config::load_from_disk()?;
    let token = config.bot_token;
    let http_client = Arc::new(Client::new(token.clone()));
    let web_client = reqwest::Client::builder()
        .user_agent("discord-threads-link-expander-bot")
        .build()?;

    let config = Config::new(token.clone(), Intents::empty());
    let mut shards = stream::create_recommended(&http_client, config, |_, builder| builder.build())
        .await?
        .collect::<Vec<_>>();

    let mut stream = ShardEventStream::new(shards.iter_mut());

    let application_id = {
        let response = http_client.current_user_application().await?;

        response.model().await?.id
    };

    while let Some((_, event)) = stream.next().await {
        match event {
            Err(error) => {
                if error.is_fatal() {
                    eprintln!("Gateway connection fatally closed, error: {error:?}");
                    break;
                }
            }
            Ok(event) => {
                match event {
                    Event::Ready(_) => {
                        // Register our commands on startup
                        let expand_threads_link = CommandBuilder::new(
                            MessageCommands::ExpandThreadsLink.as_str(),
                            "",
                            CommandType::Message,
                        )
                        .build();
                        let fix_twitter_link_message = CommandBuilder::new(
                            MessageCommands::FixTwitterLink.as_str(),
                            "",
                            CommandType::Message,
                        )
                        .build();
                        let fix_twitter_link_slash = CommandBuilder::new(
                            SlashCommands::FixTwitterLink.as_str(),
                            "twitter.com -> fxtwitter.com and etc.",
                            CommandType::ChatInput,
                        )
                        .dm_permission(true)
                        .option(CommandOption {
                            autocomplete: None,
                            channel_types: None,
                            choices: None,
                            description: "Enter a Twitter link or a message containing one \
                                            or more twitter links"
                                .to_string(),
                            kind: CommandOptionType::String,
                            name: "message".to_string(),
                            required: Some(true),
                            description_localizations: None,
                            max_length: None,
                            max_value: None,
                            min_length: None,
                            min_value: None,
                            name_localizations: None,
                            options: None,
                        })
                        .build();

                        let interaction_client = http_client.interaction(application_id);
                        interaction_client
                            .set_global_commands(&[
                                expand_threads_link,
                                fix_twitter_link_slash,
                                fix_twitter_link_message,
                            ])
                            .await?;
                    }
                    Event::InteractionCreate(interaction) => {
                        let Some(InteractionData::ApplicationCommand(command_data)) =
                            &interaction.data
                        else {
                            println!(
                            "Received an interaction that wasn't an application command, ignoring"
                        );
                            continue;
                        };

                        if let Err(e) = match &command_data.name {
                            name if name == MessageCommands::ExpandThreadsLink.as_str() => {
                                handle_expand_threads_link(
                                    &interaction,
                                    application_id,
                                    http_client.clone(),
                                    &web_client,
                                )
                                .await
                            }
                            name if name == SlashCommands::FixTwitterLink.as_str()
                                || name == MessageCommands::FixTwitterLink.as_str() =>
                            {
                                handle_fix_twitter_link(
                                    &interaction,
                                    application_id,
                                    http_client.clone(),
                                )
                                .await
                            }
                            _ => Ok(()),
                        } {
                            eprintln!(
                                "Failed to handle \"{}\" interaction: {}",
                                &command_data.name, e
                            );
                        }
                    }
                    _ => (),
                }
            }
        }
    }

    Ok(())
}
