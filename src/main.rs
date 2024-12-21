use twilight_gateway::{Message, Intents, Config, stream::{self, ShardMessageStream}};
use ed25519_dalek::{Signer, SigningKey};
use tokio::time::{sleep, Duration};
use twilight_http::Client;
use time::OffsetDateTime;
use futures::StreamExt;
use serde::Deserialize;
use std::time::Instant;
use std::error::Error;
use std::fs;
use hex;

#[derive(Deserialize)]
struct Project {
    bot_token: String,
    api_url: String,
    signing_key: String,
    api_key: String
}

async fn load_project() -> Result<Project, Box<dyn Error>> {
    let project_str: String = fs::read_to_string("project.toml")?;
    let project: Project = toml::from_str(&project_str)?;
    Ok(project)
}

fn create_signature(
    signing_key: &SigningKey,
    timestamp: &str,
    body: &str,
) -> String {
    hex::encode(
        signing_key.sign(
            format!("{}{}", timestamp, body)
            .as_bytes()
        ).to_bytes()
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let project: Project = load_project().await?;

    let signing_key: SigningKey = SigningKey::from_bytes(
        &hex::decode(&project.signing_key)?.try_into().unwrap());

    let client = reqwest::Client::new();
    let api_endpoint: String = format!("{}/discord/event", project.api_url.trim_end_matches('/'));

    let bot = Client::new(project.bot_token.clone());
    let config = Config::new(
        project.bot_token,
        Intents::GUILDS                    |
        Intents::GUILD_EMOJIS_AND_STICKERS |
        Intents::GUILD_WEBHOOKS            |
        Intents::GUILD_MESSAGES            |
        Intents::GUILD_MESSAGE_REACTIONS   |
        Intents::MESSAGE_CONTENT
    );

    let mut shards = stream::create_recommended(&bot, config, |_, builder| builder.build())
        .await?
        .collect::<Vec<_>>();

    println!("event forwarding to {} with {} shards", api_endpoint, shards.len());

    let mut stream = ShardMessageStream::new(shards.iter_mut());

    while let Some((_shard, message)) = stream.next().await {
        let message = match message {
            Ok(message) => message,
            Err(_) => {
                continue;
            }
        };

        let json_str: String = match message {
            Message::Text(content) => content,
            Message::Close(_) => {
                continue;
            }
        };

        if json_str.starts_with("{\"t\":\"READY\"") ||
           json_str.starts_with("{\"t\":null")      ||
           json_str.starts_with("{\"t\":\"RESUMED\"")
        {
            continue;
        }

        tokio::spawn(forward_event(
            client.clone(),
            api_endpoint.clone(),
            json_str,
            signing_key.clone(),
            project.api_key.clone()
        ));
    }

    Ok(())
}

async fn forward_event(
    client: reqwest::Client,
    endpoint: String,
    json_str: String,
    signing_key: SigningKey,
    api_key: String
) {
    let max_retries = 5;
    let retry_delay = Duration::from_millis(500);

    for attempt in 0..max_retries {
        let timestamp = OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();

        let signature = create_signature(&signing_key, &timestamp, &json_str);
        let start_time = Instant::now();

        match client.post(&endpoint)
            .header("Content-Type", "application/json")
            .header("X-Signature-Ed25519", signature)
            .header("X-Signature-Timestamp", timestamp)
            .header("Authorization", api_key.clone())
            .body(json_str.clone())
            .send()
            .await
        {
            Ok(response) => {
                let duration = start_time.elapsed().as_millis();

                if response.status() == reqwest::StatusCode::BAD_GATEWAY ||
                   response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE {
                    if attempt < max_retries - 1 {
                        println!("Received {}, retrying in 500ms (attempt {}/{})", response.status().as_str(), attempt + 1, max_retries);
                        sleep(retry_delay).await;
                        continue;
                    }
                    println!("Failed after {} attempts with 502 Bad Gateway", max_retries);
                    break
                }
                if !response.status().is_success() {
                    println!("api request failed with status: {}", response.status());
                    break
                }

                let event: String = response.text().await.unwrap();

                if event != "DUPLICATE_EVENT" {
                    println!("forwarded {} event (took {}ms)", event, duration);
                }
                break;
            }
            Err(e) => {
                println!("error forwarding event: {:?}", e);
                break;
            }
        }
    }
}
